//! Máquina de estados pura do painel: sem Tauri, Win32 nem relógio.
//!
//! A camada de plataforma consulta o cursor (~50 ms), traduz a posição para [`CursorZone`]
//! e chama [`Panel::step`] com o instante atual. A saída diz o que fazer; nada é executado aqui.

use std::time::{Duration, Instant};

/// Atrasos do painel.
#[derive(Debug, Clone, Copy)]
pub struct Delays {
    /// Pausa contínua na borda até abrir.
    pub open: Duration,
    /// Tempo fora do painel até fechar.
    pub close: Duration,
}

impl Default for Delays {
    fn default() -> Self {
        Self {
            open: Duration::from_millis(150),
            close: Duration::from_millis(400),
        }
    }
}

/// Onde o cursor está, já traduzido pela plataforma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorZone {
    /// Tocando a borda de ativação.
    Edge,
    /// Dentro do painel aberto.
    Inside,
    /// Em qualquer outro lugar.
    Outside,
}

/// Estado do painel. `Arming` e `Open` guardam o instante em que começou a espera.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Hidden,
    Arming {
        since: Instant,
    },
    /// `leaving_since` é `Some` enquanto o cursor está fora e o fechamento está contando.
    Open {
        leaving_since: Option<Instant>,
    },
    Pinned,
}

/// Entrada da máquina.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// Leitura periódica do cursor. `editing` indica edição ativa.
    Tick {
        zone: CursorZone,
        editing: bool,
    },
    Pin,
    Unpin,
}

/// Efeito que a camada de cima deve executar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Show,
    Hide,
}

#[derive(Debug, Clone, Copy)]
pub struct Panel {
    pub state: State,
    pub delays: Delays,
}

impl Panel {
    pub fn new(delays: Delays) -> Self {
        Self {
            state: State::Hidden,
            delays,
        }
    }

    /// Avança a máquina e devolve o efeito a executar.
    pub fn step(&mut self, input: Input, now: Instant) -> Action {
        match (self.state, input) {
            (
                State::Hidden,
                Input::Tick {
                    zone: CursorZone::Edge,
                    ..
                },
            ) => {
                self.state = State::Arming { since: now };
                Action::None
            }
            (State::Arming { since }, Input::Tick { zone, .. }) => match zone {
                CursorZone::Edge if now.duration_since(since) >= self.delays.open => {
                    self.state = State::Open {
                        leaving_since: None,
                    };
                    Action::Show
                }
                CursorZone::Edge => Action::None,
                _ => {
                    self.state = State::Hidden;
                    Action::None
                }
            },
            (State::Open { leaving_since }, Input::Tick { zone, editing }) => {
                // A borda conta como dentro: o painel está colado nela.
                let outside = zone == CursorZone::Outside && !editing;
                match (outside, leaving_since) {
                    (false, _) => {
                        self.state = State::Open {
                            leaving_since: None,
                        };
                        Action::None
                    }
                    (true, None) => {
                        self.state = State::Open {
                            leaving_since: Some(now),
                        };
                        Action::None
                    }
                    (true, Some(t)) if now.duration_since(t) >= self.delays.close => {
                        self.state = State::Hidden;
                        Action::Hide
                    }
                    (true, Some(_)) => Action::None,
                }
            }
            (State::Open { .. }, Input::Pin) => {
                self.state = State::Pinned;
                Action::None
            }
            (State::Hidden | State::Arming { .. }, Input::Pin) => {
                self.state = State::Pinned;
                Action::Show
            }
            (State::Pinned, Input::Unpin) => {
                // Reinicia a contagem: o cursor pode já estar fora.
                self.state = State::Open {
                    leaving_since: None,
                };
                Action::None
            }
            _ => Action::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const D: Delays = Delays {
        open: Duration::from_millis(150),
        close: Duration::from_millis(400),
    };

    fn tick(p: &mut Panel, zone: CursorZone, editing: bool, t0: Instant, ms: u64) -> Action {
        p.step(
            Input::Tick { zone, editing },
            t0 + Duration::from_millis(ms),
        )
    }

    fn opened(t0: Instant) -> Panel {
        let mut p = Panel::new(D);
        tick(&mut p, CursorZone::Edge, false, t0, 0);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 150), Action::Show);
        p
    }

    #[test]
    fn abertura_exige_pausa_na_borda() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        tick(&mut p, CursorZone::Edge, false, t0, 0);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 100), Action::None);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 150), Action::Show);
    }

    #[test]
    fn passagem_rapida_pela_borda_nao_abre() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        tick(&mut p, CursorZone::Edge, false, t0, 0);
        tick(&mut p, CursorZone::Outside, false, t0, 50);
        // Voltar à borda recomeça a contagem do zero.
        tick(&mut p, CursorZone::Edge, false, t0, 100);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 200), Action::None);
        assert_eq!(
            p.state,
            State::Arming {
                since: t0 + Duration::from_millis(100)
            }
        );
    }

    #[test]
    fn reentrada_cancela_fechamento() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        tick(&mut p, CursorZone::Outside, false, t0, 200);
        tick(&mut p, CursorZone::Inside, false, t0, 400);
        // Sair de novo recomeça os 400 ms.
        tick(&mut p, CursorZone::Outside, false, t0, 450);
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 800),
            Action::None
        );
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 850),
            Action::Hide
        );
    }

    #[test]
    fn fixado_e_edicao_impedem_fechamento() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        tick(&mut p, CursorZone::Outside, true, t0, 200);
        assert_eq!(
            tick(&mut p, CursorZone::Outside, true, t0, 5000),
            Action::None
        );

        p.step(Input::Pin, t0 + Duration::from_millis(5001));
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 9000),
            Action::None
        );
        assert_eq!(p.state, State::Pinned);

        // Desafixado, volta a fechar normalmente.
        p.step(Input::Unpin, t0 + Duration::from_millis(9001));
        tick(&mut p, CursorZone::Outside, false, t0, 9002);
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 9402),
            Action::Hide
        );
    }
}
