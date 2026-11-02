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

/// Quem tem o foco de teclado, já traduzido pela plataforma.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// A janela do painel.
    Panel,
    /// A página web do painel.
    Page,
    /// Outro app.
    Elsewhere,
    /// Ninguém, em transições do sistema: não prova que o foco saiu.
    Unknown,
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
    /// Aberto pelo atalho: quem manda é o foco, não o cursor. `focused` diz se ele já chegou.
    Summoned {
        focused: bool,
    },
    Pinned,
    /// Fechado à mão. A borda só volta a valer depois de o cursor sair dela; sem isso, o cursor
    /// parado ali reabriria o painel logo depois de fechar.
    Dismissed,
}

/// Entrada da máquina.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Input {
    /// Leitura periódica do cursor. `editing` indica edição ativa, na UI, na página ou no seletor
    /// nativo. `escape` é um toque novo de Esc.
    Tick {
        zone: CursorZone,
        editing: bool,
        focus: Focus,
        escape: bool,
    },
    Pin,
    Unpin,
    /// Atalho global. `dialog` indica o seletor nativo aberto.
    Shortcut {
        dialog: bool,
    },
    /// O Windows confirmou o foco pedido em `Summon`.
    Activated,
}

/// Efeito que a camada de cima deve executar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Show,
    /// Mostra e dá o foco de teclado ao painel.
    Summon,
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

    fn dismiss(&mut self) -> Action {
        self.state = State::Dismissed;
        Action::Hide
    }

    /// Avança a máquina e devolve o efeito a executar.
    pub fn step(&mut self, input: Input, now: Instant) -> Action {
        match (self.state, input) {
            // O seletor é modal: ao fechar, ele religaria a janela do painel já oculto.
            (_, Input::Shortcut { dialog: true }) => Action::None,
            (State::Hidden | State::Arming { .. } | State::Dismissed, Input::Shortcut { .. }) => {
                self.state = State::Summoned { focused: false };
                Action::Summon
            }
            (State::Open { .. } | State::Summoned { .. }, Input::Shortcut { .. }) => self.dismiss(),
            (State::Dismissed, Input::Tick { zone, .. }) => {
                if zone != CursorZone::Edge {
                    self.state = State::Hidden;
                }
                Action::None
            }
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
            (State::Open { leaving_since }, Input::Tick { zone, editing, .. }) => {
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
            (
                State::Summoned { focused },
                Input::Tick {
                    editing,
                    focus,
                    escape,
                    ..
                },
            ) => match focus {
                Focus::Panel if escape && !editing => self.dismiss(),
                Focus::Panel | Focus::Page => {
                    self.state = State::Summoned { focused: true };
                    Action::None
                }
                // Fora do painel, só o seletor nativo conta como edição.
                Focus::Elsewhere if editing => Action::None,
                Focus::Elsewhere if focused => self.dismiss(),
                // A ativação falhou. Sem foco a perder, vale a regra do cursor: assim o painel não
                // fica preso sobre as outras janelas.
                Focus::Elsewhere => {
                    self.state = State::Open {
                        leaving_since: None,
                    };
                    Action::None
                }
                Focus::Unknown => Action::None,
            },
            (State::Summoned { .. }, Input::Activated) => {
                self.state = State::Summoned { focused: true };
                Action::None
            }
            (State::Open { .. } | State::Summoned { .. }, Input::Pin) => {
                self.state = State::Pinned;
                Action::None
            }
            (State::Hidden | State::Arming { .. } | State::Dismissed, Input::Pin) => {
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
            Input::Tick {
                zone,
                editing,
                focus: Focus::Elsewhere,
                escape: false,
            },
            t0 + Duration::from_millis(ms),
        )
    }

    /// Consulta com o cursor fora, que é o caso em que o atalho difere da abertura pelo cursor.
    fn focus_tick(p: &mut Panel, focus: Focus, editing: bool, escape: bool, t0: Instant) -> Action {
        let tick = Input::Tick {
            zone: CursorZone::Outside,
            editing,
            focus,
            escape,
        };
        p.step(tick, t0)
    }

    fn shortcut(p: &mut Panel, t0: Instant) -> Action {
        p.step(Input::Shortcut { dialog: false }, t0)
    }

    fn summoned(t0: Instant) -> Panel {
        let mut p = Panel::new(D);
        assert_eq!(shortcut(&mut p, t0), Action::Summon);
        assert_eq!(
            focus_tick(&mut p, Focus::Panel, false, false, t0),
            Action::None
        );
        p
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

    #[test]
    fn atalho_abre_durante_a_pausa_na_borda() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        tick(&mut p, CursorZone::Edge, false, t0, 0);
        assert_eq!(shortcut(&mut p, t0), Action::Summon);
    }

    #[test]
    fn aberto_pelo_atalho_ignora_o_cursor_fora() {
        let t0 = Instant::now();
        let mut p = summoned(t0);
        for ms in [0, 1000, 60_000] {
            let at = t0 + Duration::from_millis(ms);
            assert_eq!(
                focus_tick(&mut p, Focus::Panel, false, false, at),
                Action::None
            );
        }
    }

    #[test]
    fn esc_so_fecha_com_foco_no_painel_e_sem_campo_em_edicao() {
        let t0 = Instant::now();
        let mut p = summoned(t0);
        assert_eq!(
            focus_tick(&mut p, Focus::Panel, true, true, t0),
            Action::None
        );
        assert_eq!(
            focus_tick(&mut p, Focus::Page, true, true, t0),
            Action::None
        );
        assert_eq!(
            focus_tick(&mut p, Focus::Panel, false, true, t0),
            Action::Hide
        );
    }

    #[test]
    fn foco_em_outro_app_fecha_e_transicao_do_sistema_nao() {
        let t0 = Instant::now();
        let mut p = summoned(t0);
        assert_eq!(
            focus_tick(&mut p, Focus::Page, true, false, t0),
            Action::None
        );
        assert_eq!(
            focus_tick(&mut p, Focus::Unknown, false, false, t0),
            Action::None
        );
        // O seletor nativo fica em primeiro plano, mas é do painel.
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, true, false, t0),
            Action::None
        );
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, false, false, t0),
            Action::Hide
        );
    }

    #[test]
    fn foco_perdido_antes_da_consulta_seguinte_fecha_o_painel() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        shortcut(&mut p, t0);
        p.step(Input::Activated, t0);
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, false, false, t0),
            Action::Hide
        );
    }

    #[test]
    fn ativacao_que_falhou_cai_na_regra_do_cursor() {
        let t0 = Instant::now();
        let mut p = Panel::new(D);
        shortcut(&mut p, t0);
        assert_eq!(
            focus_tick(&mut p, Focus::Elsewhere, false, false, t0),
            Action::None
        );
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 0),
            Action::None
        );
        assert_eq!(
            tick(&mut p, CursorZone::Outside, false, t0, 400),
            Action::Hide
        );
    }

    #[test]
    fn atalho_fecha_e_cursor_na_borda_so_reabre_depois_de_sair_dela() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        assert_eq!(shortcut(&mut p, t0), Action::Hide);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 200), Action::None);
        assert_eq!(tick(&mut p, CursorZone::Edge, false, t0, 900), Action::None);
        assert_eq!(p.state, State::Dismissed);

        tick(&mut p, CursorZone::Outside, false, t0, 950);
        tick(&mut p, CursorZone::Edge, false, t0, 1000);
        assert_eq!(
            tick(&mut p, CursorZone::Edge, false, t0, 1150),
            Action::Show
        );
    }

    #[test]
    fn atalho_nao_fecha_fixado_nem_com_o_seletor_aberto() {
        let t0 = Instant::now();
        let mut p = opened(t0);
        assert_eq!(p.step(Input::Shortcut { dialog: true }, t0), Action::None);
        p.step(Input::Pin, t0);
        assert_eq!(shortcut(&mut p, t0), Action::None);
        assert_eq!(p.state, State::Pinned);
    }
}
