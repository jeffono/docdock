# 0004 — Conteúdo não confiável na janela principal

**Contexto:** a M1 vai renderizar markdown na própria UI, que fala com o Rust. Uma nota pode vir de
terceiros, por sincronização ou download. HTML injetado na janela `main` alcançaria todos os comandos, e
abrir uma nota não pode acionar a rede. No Tauri 2.12.5, o asset protocol tem escopo amplo, a CSP não
impede navegação e `tauri.conf.json` não oferece controle de navegação por janela.

**Decisão:**

- **Markdown:** convertido no Rust com `pulldown-cmark` e sanitizado com `ammonia`, em lista permitida. A
  UI só recebe HTML já sanitizado, e esse é o único uso permitido de `{@html}`. Texto cru nunca vai ao DOM
  como HTML.
- **Imagens locais:** protocolo próprio `note-asset`, em vez do asset protocol. Ele só serve png, jpg,
  jpeg, gif e webp dentro da pasta de notas, com as garantias de `resolve`. A extensão vale para o pedido
  e para o destino canônico. SVG fica de fora, por poder conter script. O tipo vem da extensão, com
  `nosniff` e `no-store`.
- **Só a janela `main` lê imagens:** o protocolo é registrado em todos os WebViews, inclusive no da janela
  `web`, e a barreira de `lib.rs` só cobre comandos. O pedido traz o rótulo do WebView, e qualquer outro
  recebe 403. Sem isso, um site saberia quais imagens existem e poderia exibi-las.
- **Imagens remotas:** bloqueadas pela CSP, por privacidade.
- **CSP:** `default-src 'none'`, liberando só `'self'` para script, estilo e imagem, o `note-asset` e o
  IPC em `http://ipc.localhost`. Sem `'unsafe-inline'`, `dangerousDisableAssetCspModification` ou
  `devCsp`.
- **Navegação:** `main` é criada em `main_window.rs` com `create: false`, porque só o construtor aceita
  regra por janela. Ela navega apenas na origem da UI, `http://tauri.localhost` no build final e `devUrl`
  em dev, e nunca abre outra janela.
- **`Object.freeze(Object.prototype)`:** injetado só em `main`. O `freezePrototype` do Tauri roda em todo
  WebView e quebraria sites na janela `web`.
- **Capabilities:** continuam só `core:default`. O seletor de pasta é chamado pelo Rust, e os comandos de
  `tauri-plugin-dialog` não são concedidos à UI.

**Consequências:**

- A CSP só existe no build final. Em dev no desktop, a página vem direto do Vite, sem passar pelo
  protocolo do Tauri que adiciona o cabeçalho, e `devCsp` não teria efeito. Teste segurança e estilo no
  build final.
- A UI não pode usar atributo `style` no HTML, `<style>` em tempo de execução, `eval` ou `data:`. Definir
  estilo pelo CSSOM funciona, e o CSS do Svelte sai em arquivo, então não há nonce de `<style>`.
- O `<form>` da UI precisa chamar `preventDefault`: `form-action 'none'` bloqueia o envio real.
- A CSP cobre subframes e `on_navigation` cobre o frame principal. Esse controle falha aberto para uma URL
  que o crate `url` não consiga interpretar, o que o Chromium não deveria produzir.
- Links em conteúdo renderizado não navegam. Para abri-los no navegador padrão, a UI intercepta o clique
  e chama um comando que valida http(s) e mailto. Nunca afrouxe `on_navigation` nem `on_new_window` para
  isso.
- O caminho de `note-asset` é relativo à pasta de notas, com percent-encoding. O renderizador reescreve os
  `src` para `http://note-asset.localhost/<caminho>`.
