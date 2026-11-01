# 0003 — Tauri aprovado na M0, com página web em janela própria

**Contexto:** a M0 existia para provar o painel antes de construir o resto. Foco, animação, dois
monitores e páginas web foram testados no app real, e o usuário aprovou a experiência. Vários WebViews
na mesma janela seguem atrás da feature `unstable` no Tauri 2.12.5, e um iframe não serve porque muitos
sites o bloqueiam.

**Decisão:**

- Seguir com o Tauri; o Electron deixa de ser alternativa ativa.
- Exibir páginas web em uma janela própria, possuída pelo painel, sem `unstable`.
- Mover as duas janelas na mesma transação de `DeferWindowPos`.
- Tratar a página como não confiável: fora de qualquer capability, só http(s) e nenhuma janela nova.

**Consequências:**

- O `invoke` de origem remota é recusado pelo ACL do Tauri, mas `__TAURI_INTERNALS__` existe na página.
  Nunca conceda capability `remote` a essa janela.
- A página compartilha o perfil WebView2 da UI; só o processo de renderização é separado.
- `add_child` exigiria tirar `WS_EX_NOACTIVATE` da janela principal, por isso foi
  descartado.
- A página não fala com o Rust: foco e Esc são observados pela thread do painel.
- Sem página web, o app usa cerca de 160 MB de memória privada oculto; a página soma o que o site exigir
  (github.com passou de 450 MB em desenvolvimento).
