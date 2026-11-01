//! Camada fina sobre o Win32. Todas as coordenadas são pixels físicos: o processo roda com
//! consciência de DPI por monitor (PMv2), ativada pelo tao ao iniciar o laço de eventos.

use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Graphics::Gdi::{
    CreateRectRgn, DeleteObject, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
    SetWindowRgn,
};
use windows::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, GetAsyncKeyState, VK_ESCAPE};
use windows::Win32::UI::WindowsAndMessaging::{
    BeginDeferWindowPos, DeferWindowPos, EndDeferWindowPos, GWL_EXSTYLE, GetCursorPos,
    GetForegroundWindow, GetShellWindow, GetWindowLongPtrW, HWND_TOPMOST, IsWindow, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOOWNERZORDER, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, WS_EX_APPWINDOW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};
use windows::core::BOOL;

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

/// Se o Esc está pressionado ou foi tocado desde a última leitura. Leia a cada consulta do cursor:
/// assim um toque curto não escapa entre duas leituras e o bit de "desde a última" nunca fica velho.
pub fn escape_pressed() -> bool {
    unsafe { GetAsyncKeyState(i32::from(VK_ESCAPE.0)) as u16 & 0x8001 != 0 }
}

unsafe extern "system" fn collect_monitor(
    handle: HMONITOR,
    _: HDC,
    _: *mut RECT,
    list: LPARAM,
) -> BOOL {
    unsafe { &mut *(list.0 as *mut Vec<HMONITOR>) }.push(handle);
    true.into()
}

/// Todos os monitores conectados.
pub fn monitors() -> Vec<Monitor> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    let list = LPARAM(&raw mut handles as isize);
    let _ = unsafe { EnumDisplayMonitors(None, None, Some(collect_monitor), list) };
    handles.into_iter().filter_map(monitor_info).collect()
}

fn monitor_info(handle: HMONITOR) -> Option<Monitor> {
    unsafe {
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

/// Janela em primeiro plano. `None` em transições do sistema.
pub fn foreground_window() -> Option<isize> {
    let raw = unsafe { GetForegroundWindow() }.0 as isize;
    (raw != 0).then_some(raw)
}

/// Como uma janela do painel recebe o foco.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Activation {
    /// Nunca pelo clique: a UI pede com [`PanelWindow::activate`].
    OnRequest,
    /// Pelo clique, como qualquer janela. É o caso da página web, que não fala com o Rust.
    OnClick,
}

/// Uma janela do painel. Guarda o HWND como inteiro porque a thread do painel é dona dela.
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
    /// Aplica os estilos e mostra a janela já no tamanho final, na posição `x` e sem ativar.
    pub fn prepare(
        raw: isize,
        rect: Rect,
        clip_right: i32,
        x: i32,
        activation: Activation,
    ) -> Self {
        let window = Self {
            raw,
            clip_right,
            top: rect.top,
            width: rect.right - rect.left,
            height: rect.bottom - rect.top,
        };
        let hwnd = window.hwnd();
        window.set_enabled(false);
        window.clip(x);
        unsafe {
            let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
            let style = (style | WS_EX_TOOLWINDOW.0) & !WS_EX_APPWINDOW.0;
            let style = match activation {
                Activation::OnRequest => style | WS_EX_NOACTIVATE.0,
                Activation::OnClick => style,
            };
            SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style as isize);
            let result = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                rect.top,
                window.width,
                window.height,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            log_failure("SetWindowPos", result);
        }
        window
    }

    fn hwnd(&self) -> HWND {
        HWND(self.raw as *mut _)
    }

    pub fn exists(&self) -> bool {
        unsafe { IsWindow(Some(self.hwnd())) }.as_bool()
    }

    /// Volta ao topo das janelas sempre visíveis, sem ativar. Janelas possuídas sobem junto.
    pub fn raise(&self) {
        let flags = SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOMOVE;
        let result = unsafe { SetWindowPos(self.hwnd(), Some(HWND_TOPMOST), 0, 0, 0, 0, flags) };
        log_failure("SetWindowPos", result);
    }

    /// Recorta o que passar da borda, para nunca aparecer em um monitor vizinho.
    fn clip(&self, x: i32) {
        let visible = (self.clip_right - x).clamp(0, self.width);
        unsafe {
            let region = CreateRectRgn(0, 0, visible, self.height);
            // O sistema só vira dono da região se a chamada der certo.
            if SetWindowRgn(self.hwnd(), Some(region), false) == 0 {
                let _ = DeleteObject(region.into());
            }
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

    pub fn activate(&self) {
        let activated = unsafe { SetForegroundWindow(self.hwnd()) }.as_bool();
        eprintln!("[painel] ativar: ok={activated}");
    }
}

/// Move as janelas para `x` em uma só transação, para a página não descolar do painel.
pub fn slide_windows(windows: &[PanelWindow], x: i32) {
    let flags = SWP_NOACTIVATE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOOWNERZORDER;
    let moved = || unsafe {
        let mut batch = BeginDeferWindowPos(windows.len() as i32)?;
        for window in windows {
            window.clip(x);
            batch = DeferWindowPos(batch, window.hwnd(), None, x, window.top, 0, 0, flags)?;
        }
        EndDeferWindowPos(batch)
    };
    log_failure("DeferWindowPos", moved());
}

/// Devolve o foco à janela guardada ou, se ela não existe mais, à área de trabalho, para o teclado
/// não ficar em um painel oculto.
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
