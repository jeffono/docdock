# Plano

Cada tarefa indica o nível de modelo (ver `AGENTS.md`). Marque `[x]` no commit que a conclui e detalhe
cada etapa quando ela for a próxima.

## Em andamento

Nada.

## Antes da M2

- [ ] (básico) Migrar para o inglês documentação, plano, comentários e nomes de testes; o histórico de
  commits fica como está

## Concluídas

- M0 — Painel provado e aprovado com o Tauri; ver `docs/decisions/0003-painel-e-paginas-web.md`.
- M1 — Consulta de notas em markdown, bandeja, atalho global, CSP e identidade visual; ver `docs/DESIGN.md`.

## Etapa atual: M2 — Criar e editar

- [ ] (avançado) Lado da tela configurável pela bandeja, direito por padrão; independe do resto
- [ ] (padrão) Internacionalização da interface e dos textos do Rust: inglês padrão, pt-BR pelo idioma do
  Windows; erros do Rust viram códigos
- [ ] (avançado) Gravação atômica e autosave
- [ ] (avançado) Reação a alterações externas, sem perda de conteúdo
- [ ] (padrão) Frontmatter: esconder o bloco e mostrar título e tags
- [ ] (padrão) Editor CodeMirror 6 com formatação ao vivo: títulos, ênfase e marcações escondidas fora da
  linha atual. Sem WYSIWYG, que reescreveria o arquivo
- [ ] (padrão) Colar imagens como anexos

## M3 — Encontrar e fixar

- [ ] (padrão) Busca no conteúdo com índice em memória; FTS5 só se medir lentidão
- [ ] (padrão) Barra única: buscar notas e criar nota com o texto digitado, como no Notational Velocity
- [ ] (padrão) Notas fixadas em acordeão no topo, abertas por padrão; ver
  `docs/decisions/0005-itens-fixados.md`
- [ ] (padrão) Restaurar `_pinned.md` apagado

## M4 — Páginas web

- [ ] (padrão) Navegador no lugar das notas, com botão de voltar; a página fica escondida, não fechada
- [ ] (padrão) Barra única como endereço, com histórico local e sugestões; ver
  `docs/decisions/0006-sugestoes-de-busca.md`
- [ ] (padrão) Fixar páginas em `_pinned.md`
- [ ] (padrão) Salvar páginas como notas com `url:` no frontmatter
- [ ] (padrão) Cores no código com os analisadores do CodeMirror
- [ ] (avançado) Testar se iframes da página disparam esquemas externos sem passar por `on_navigation`

## M5 — Distribuir

- [ ] (básico) README em inglês, com link para a versão em pt-BR
- [ ] (padrão) Instalador, atualização automática e definição sobre assinatura de código
- [ ] (avançado) Pôr entre aspas o caminho do autostart: `auto-launch` grava o `Run` sem aspas

## Depois

Sem ordem definida:

- Equações em MathML com Temml; KaTeX depende de estilo inline, bloqueado pela CSP.
- Diagramas Mermaid: exigem decisão sobre a CSP, que bloqueia o `<style>` que ele injeta.
- Post-its como janelas soltas na área de trabalho.
- Captura de tela.
- Ditado com modelos locais, baixados à parte do instalador.
- Plugins: exigem uma decisão registrada antes de começar.
- macOS e Linux: nova implementação de `platform/`. No Wayland, o painel depende de layer-shell, que o
  GNOME não oferece.

## Questões em aberto

Valores padrão usados até você decidir o contrário.

| Questão                         | Padrão                                                                            |
| ------------------------------- | --------------------------------------------------------------------------------- |
| Monitor                         | O mais à direita, cuja borda segura o cursor                                      |
| Fechar ao tirar o mouse         | Sim, exceto se fixado, com edição ativa ou com a página em foco                   |
| Página sobre apps em tela cheia | Funciona em janela e tela cheia sem bordas; jogos em modo exclusivo ficam de fora |
