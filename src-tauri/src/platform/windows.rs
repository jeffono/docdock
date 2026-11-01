//! Camada fina sobre o Win32. Todas as coordenadas são pixels físicos: o processo roda com
//! consciência de DPI por monitor (PMv2), ativada pelo tao ao iniciar o laço de eventos.

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    CreateRectRgn, GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromPoint,
    SetWindowRgn,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::EnableWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetCursorPos, GetForegroundWindow, GetShellWindow, GetWindowLongPtrW,
    HWND_TOPMOST, IsWindow, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE,
    SWP_NOZORDER, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos,
    WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

/// Retângulo meio aberto: `right` e `bottom` ficam fora.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Rect {
    pub fn contains(self, p: Point) -> bool {
        (self.left..self.right).contains(&p.x) && (self.top..self.bottom).contains(&p.y)
    }
}

impl From<RECT> for Rect {
    fn from(r: RECT) -> Self {
        Self {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Monitor {
    pub bounds: Rect,
    pub work: Rect,
    /// DPI efetivo do monitor.
    pub dpi: u32,
}

/// `None` quando o cursor não pode ser lido, como na tela de UAC.
pub fn cursor_pos() -> Option<Point> {
    let mut p = POINT::default();
    unsafe { GetCursorPos(&mut p) }.ok()?;
    Some(Point { x: p.x, y: p.y })
}

/// O monitor principal é, por definição, o que contém a origem.
pub fn primary_monitor() -> Option<Monitor> {
    unsafe {
        let handle = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
        let mut info = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        GetMonitorInfoW(handle, &mut info).ok().ok()?;
        let (mut dpi, mut unused) = (0, 0);
        GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &mut dpi, &mut unused).ok()?;
        Some(Monitor {
            bounds: info.rcMonitor.into(),
            work: info.rcWork.into(),
            dpi,
        })
    }
}

/// Bloqueia até a próxima composição do DWM, para cada quadro cair em um vsync.
pub fn wait_frame() {
    let _ = unsafe { DwmFlush() };
}

/// A janela do painel. Guarda o HWND como inteiro porque a thread do painel é dona dela.
#[derive(Debug, Clone, Copy)]
pub struct PanelWindow {
    raw: isize,
    /// Tudo o que passar desta coordenada X fica recortado.
    clip_right: i32,
    top: i32,
    width: i32,
    height: i32,
}

impl PanelWindow {
    /// Aplica os estilos e mostra a janela já no tamanho final, fora da tela e sem ativar.
    pub fn prepare(raw: isize, rect: Rect, clip_right: i32, parked_x: i32) -> Self {
        let window = Self {
            raw,
            clip_right,
            top: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        };
        let hwnd = window.hwnd();
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let style = (style | WS_EX_NOACTIVATE.0 | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
            let result = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                parked_x,
                rect.top,
                window.width,
                window.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            log_failure("SetWindowPos", result);
        }
        window.set_x(parked_x);
        window.set_enabled(false);
        window
    }

    fn hwnd(&self) -> HWND {
        HWND(self.raw as *mut _)
    }

    /// Volta ao topo das janelas sempre visíveis, sem ativar.
    pub fn raise(&self) {
        let flags = SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE;
        let result = unsafe { SetWindowPos(self.hwnd(), Some(HWND_TOPMOST), 0, 0, 0, 0, flags) };
        log_failure("SetWindowPos", result);
    }

    /// Move a janela e recorta o que passar da borda, para nunca aparecer em um monitor vizinho.
    pub fn set_x(&self, x: i32) {
        let visible = (self.clip_right - x).clamp(0, self.width);
        let flags = SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER;
        unsafe {
            let region = CreateRectRgn(0, 0, visible, self.height);
            // O sistema passa a ser dono da região.
            SetWindowRgn(self.hwnd(), Some(region), false);
            let result = SetWindowPos(self.hwnd(), None, x, self.top, 0, 0, flags);
            log_failure("SetWindowPos", result);
        }
    }

    /// Janela desabilitada não recebe ativação. Sem isso, o painel fora da tela herda o foco quando
    /// o app em primeiro plano fecha, porque o Windows escolhe a próxima da ordem Z, mesmo com
    /// `WS_EX_NOACTIVATE`, e o teclado iria para uma janela invisível.
    pub fn set_enabled(&self, enabled: bool) {
        let _ = unsafe { EnableWindow(self.hwnd(), enabled) };
    }

    pub fn is_foreground(&self) -> bool {
        unsafe { GetForegroundWindow() }.0 as isize == self.raw
    }

    /// Ativa o painel e devolve quem tinha o foco, para restaurar depois.
    pub fn activate(&self) -> Option<isize> {
        let previous = unsafe { GetForegroundWindow() }.0 as isize;
        if previous == self.raw {
            return None;
        }
        let activated = unsafe { SetForegroundWindow(self.hwnd()) }.as_bool();
        eprintln!("[painel] ativar: anterior={previous:#x} ok={activated}");
        Some(previous)
    }
}

/// Devolve o foco à janela guardada por [`PanelWindow::activate`] ou, se ela não existe mais, à
/// área de trabalho, para o teclado não ficar em um painel oculto.
pub fn restore_focus(previous: Option<isize>) {
    unsafe {
        let hwnd = previous
            .map(|raw| HWND(raw as *mut _))
            .filter(|&hwnd| IsWindow(Some(hwnd)).as_bool())
            .unwrap_or_else(|| GetShellWindow());
        let restored = SetForegroundWindow(hwnd).as_bool();
        eprintln!(
            "[painel] restaurar: janela={:#x} ok={restored}",
            hwnd.0 as isize
        );
    }
}

fn log_failure(call: &str, result: windows::core::Result<()>) {
    if let Err(error) = result {
        eprintln!("[painel] {call} falhou: {error}");
    }
}
