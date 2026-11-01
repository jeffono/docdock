//! Liga a máquina de estados à janela: uma única thread consulta o cursor, recebe os pedidos da
//! UI e anima o deslize. Por ser uma só, não há disputa entre abrir, fechar e animar.

use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::panel::{Action, CursorZone, Delays, Input, Panel, State};
use crate::platform::{self, Activation, Monitor, PanelWindow, Point, Rect};

const POLL: Duration = Duration::from_millis(50);
const SLIDE: Duration = Duration::from_millis(150);
/// Largura em pixels lógicos; no monitor vira `largura * dpi / 96`.
const PANEL_WIDTH: u32 = 420;
const BASE_DPI: u32 = 96;
/// Altura da barra acima da página, em pixels lógicos.
const WEB_TOP: u32 = 150;

/// Pedidos da UI para a thread do painel.
#[derive(Debug, Clone, Copy)]
pub enum Request {
    Activate,
    SetPinned(bool),
    SetEditing(bool),
    /// A janela da página web foi criada, ainda invisível.
    AttachWeb(isize),
}

/// Retângulo do painel aberto, colado à direita da área de trabalho.
fn panel_rect(monitor: &Monitor) -> Rect {
    let width = (PANEL_WIDTH * monitor.dpi / BASE_DPI) as i32;
    Rect {
        left: monitor.work.right - width,
        ..monitor.work
    }
}

/// Retângulo da página web: o painel, menos a barra do topo.
fn web_rect(monitor: &Monitor, panel: Rect) -> Rect {
    Rect {
        top: panel.top + (WEB_TOP * monitor.dpi / BASE_DPI) as i32,
        ..panel
    }
}

/// Traduz a posição do cursor. Pontos fora do monitor nunca abrem nem seguram o painel.
fn cursor_zone(cursor: Point, monitor: Rect, panel: Rect) -> CursorZone {
    if !monitor.contains(cursor) {
        CursorZone::Outside
    } else if cursor.x == monitor.right - 1 {
        CursorZone::Edge
    } else if panel.contains(cursor) {
        CursorZone::Inside
    } else {
        CursorZone::Outside
    }
}

/// Posição do deslize depois de `elapsed`, desacelerando no fim. Depende do tempo real, não da
/// contagem de quadros: um quadro atrasado salta adiante em vez de alongar a animação.
fn slide_x(from: i32, to: i32, elapsed: Duration) -> i32 {
    let t = (elapsed.as_secs_f64() / SLIDE.as_secs_f64()).min(1.0);
    let eased = 1.0 - (1.0 - t).powi(3);
    from + (f64::from(to - from) * eased).round() as i32
}

struct Slide {
    from: i32,
    to: i32,
    started: Instant,
    last_frame: Instant,
    frames: u32,
    longest_frame: Duration,
}

impl Slide {
    fn new(from: i32, to: i32) -> Self {
        let now = Instant::now();
        Self {
            from,
            to,
            started: now,
            last_frame: now,
            frames: 0,
            longest_frame: Duration::ZERO,
        }
    }

    /// Avança um quadro e devolve a posição e se terminou.
    fn advance(&mut self, now: Instant) -> (i32, bool) {
        self.frames += 1;
        self.longest_frame = self.longest_frame.max(now - self.last_frame);
        self.last_frame = now;
        let elapsed = now - self.started;
        (slide_x(self.from, self.to, elapsed), elapsed >= SLIDE)
    }

    fn report(&self, label: &str) {
        eprintln!(
            "[painel] {label}: {:.1} ms, {} quadros, maior intervalo {:.1} ms",
            self.started.elapsed().as_secs_f64() * 1000.0,
            self.frames,
            self.longest_frame.as_secs_f64() * 1000.0,
        );
    }
}

struct Dock {
    window: PanelWindow,
    /// Janela da página web, enquanto houver uma.
    web: Option<PanelWindow>,
    monitor: Monitor,
    rect: Rect,
    parked_x: i32,
    x: i32,
    panel: Panel,
    editing: bool,
    /// Último app em primeiro plano fora do painel, para devolver o foco ao fechar.
    previous_focus: Option<isize>,
    slide: Option<Slide>,
}

impl Dock {
    fn handle(&mut self, request: Request) {
        let now = Instant::now();
        let action = match request {
            Request::Activate => {
                if !self.window.is_foreground() {
                    self.window.activate();
                }
                Action::None
            }
            Request::SetEditing(editing) => {
                eprintln!("[painel] editing={editing}");
                self.editing = editing;
                Action::None
            }
            Request::AttachWeb(raw) => {
                let rect = web_rect(&self.monitor, self.rect);
                let right = self.monitor.bounds.right;
                let web = PanelWindow::prepare(raw, rect, right, self.x, Activation::OnClick);
                web.set_enabled(matches!(
                    self.panel.state,
                    State::Open { .. } | State::Pinned
                ));
                self.web = Some(web);
                Action::None
            }
            Request::SetPinned(true) => self.panel.step(Input::Pin, now),
            Request::SetPinned(false) => self.panel.step(Input::Unpin, now),
        };
        self.apply(action);
    }

    fn poll(&mut self) {
        let Some(cursor) = platform::cursor_pos() else {
            return;
        };
        self.track_focus();
        let zone = cursor_zone(cursor, self.monitor.bounds, self.rect);
        let tick = Input::Tick {
            zone,
            editing: self.editing,
        };
        let action = self.panel.step(tick, Instant::now());
        self.apply(action);
    }

    /// As janelas do painel. A da página some sozinha se for destruída, como por Alt+F4.
    fn windows(&mut self) -> Vec<PanelWindow> {
        self.web = self.web.filter(PanelWindow::exists);
        std::iter::once(self.window).chain(self.web).collect()
    }

    fn is_foreground(&mut self) -> bool {
        self.windows().iter().any(PanelWindow::is_foreground)
    }

    /// A página recebe o foco direto do Windows, sem passar pela UI: por isso o painel observa quem
    /// está em primeiro plano, em vez de guardar a janela anterior só ao pedir a ativação.
    fn track_focus(&mut self) {
        if !self.is_foreground() {
            self.previous_focus = platform::foreground_window().or(self.previous_focus);
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        for window in self.windows() {
            window.set_enabled(enabled);
        }
    }

    fn apply(&mut self, action: Action) {
        match action {
            Action::None => {}
            Action::Show => {
                self.set_enabled(true);
                self.window.raise();
                self.slide = Some(Slide::new(self.x, self.rect.left));
            }
            Action::Hide => {
                // Se o usuário já clicou em outro app, o foco é dele e não se toca nele.
                if self.is_foreground() {
                    platform::restore_focus(self.previous_focus);
                }
                self.set_enabled(false);
                self.slide = Some(Slide::new(self.x, self.parked_x));
            }
        }
    }

    /// Desenha um quadro, se houver deslize em curso.
    fn animate(&mut self) {
        let windows = self.windows();
        let Some(slide) = &mut self.slide else {
            return;
        };
        let (x, done) = slide.advance(Instant::now());
        platform::slide_windows(&windows, x);
        self.x = x;
        if done {
            let label = if x == self.parked_x {
                "fechar"
            } else {
                "abrir"
            };
            slide.report(label);
            self.slide = None;
        }
    }

    fn run(mut self, requests: Receiver<Request>) {
        let mut next_poll = Instant::now();
        loop {
            if self.slide.is_some() {
                platform::wait_frame();
                while let Ok(request) = requests.try_recv() {
                    self.handle(request);
                }
            } else {
                match requests.recv_timeout(next_poll.saturating_duration_since(Instant::now())) {
                    Ok(request) => self.handle(request),
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            if Instant::now() >= next_poll {
                self.poll();
                next_poll = Instant::now() + POLL;
            }
            self.animate();
        }
    }
}

/// Prepara a janela fora da tela e inicia a thread do painel.
pub fn start(raw_window: isize, requests: Receiver<Request>) {
    let monitor = platform::primary_monitor().expect("monitor principal indisponível");
    let rect = panel_rect(&monitor);
    let parked_x = monitor.bounds.right;
    let right = monitor.bounds.right;
    let window = PanelWindow::prepare(raw_window, rect, right, parked_x, Activation::OnRequest);
    eprintln!("[painel] monitor {monitor:?}, painel {rect:?}");
    let dock = Dock {
        window,
        web: None,
        monitor,
        rect,
        parked_x,
        x: parked_x,
        panel: Panel::new(Delays::default()),
        editing: false,
        previous_focus: None,
        slide: None,
    };
    std::thread::spawn(move || dock.run(requests));
}

#[cfg(test)]
mod tests {
    use super::*;

    const PRIMARY: Rect = Rect {
        left: 0,
        top: 0,
        right: 3840,
        bottom: 2160,
    };
    const PANEL: Rect = Rect {
        left: 3315,
        top: 47,
        right: 3840,
        bottom: 2100,
    };

    fn zone(x: i32, y: i32) -> CursorZone {
        cursor_zone(Point { x, y }, PRIMARY, PANEL)
    }

    #[test]
    fn so_o_ultimo_pixel_do_monitor_principal_e_borda() {
        assert_eq!(zone(3839, 0), CursorZone::Edge);
        assert_eq!(zone(3839, 2159), CursorZone::Edge);
        assert_eq!(zone(3838, 0), CursorZone::Outside);
    }

    #[test]
    fn pontos_em_outros_monitores_sao_ignorados() {
        // Monitor à esquerda e um eventual vizinho à direita, sob o painel estacionado.
        assert_eq!(zone(-1, 500), CursorZone::Outside);
        assert_eq!(zone(3840, 500), CursorZone::Outside);
    }

    #[test]
    fn deslize_termina_exatamente_no_destino_mesmo_com_quadro_atrasado() {
        assert_eq!(slide_x(3840, 3315, Duration::ZERO), 3840);
        assert_eq!(slide_x(3840, 3315, SLIDE), 3315);
        assert_eq!(slide_x(3840, 3315, SLIDE * 3), 3315);
    }
}
