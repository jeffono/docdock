# Plano

Cada tarefa indica o nível de modelo (ver `AGENTS.md`). Marque `[x]` no commit que a conclui e detalhe
cada etapa quando ela for a próxima.

## Em andamento

M0: deslize medido (152 a 177 ms), falta o usuário aprovar a olho. DPIs diferentes: os dois monitores
têm 120 DPI, então o caso não pôde ser testado. Página web: janela própria venceu a comparação, falta
o usuário aprovar antes do registro em `docs/decisions/0003-*.md`.

## Etapa atual: M0 — Provar o painel

**Objetivo:** responder, com um protótipo descartável, se o Tauri entrega a experiência de painel.

- [x] (padrão) Máquina de estados pura do painel, com testes
- [x] (avançado) Gatilho de borda por consulta ao cursor, sem janela na borda
- [x] (avançado) Janela sobre as demais, fora da barra de tarefas e do Alt+Tab, sem roubar foco ao abrir
- [x] (avançado) Clique no painel ativa a digitação; ao sair, o foco volta ao app anterior
- [ ] (avançado) Animação de deslize sem engasgos visíveis
- [ ] (avançado) Dois monitores com DPIs diferentes
- [x] (avançado) Página web real exibida com o painel: comparar janela própria com a feature `unstable`
- [x] (básico) Medir o uso de memória com o painel aberto e oculto
- [ ] (padrão) Registrar o resultado em `docs/decisions/0003-*.md`, incluindo a escolha para páginas web

**Memória (release):** oculto 160 MB, aberto 167 MB (privada, 7 processos; média de 3 amostras; abertura não confirmada por janela).

**Pronto quando:** você usar o protótipo com seus apps normais e aprovar a sensação do painel. Se
foco, animação ou WebView forem inviáveis, reavaliamos o Electron antes de construir o resto.

**Fato verificado:** no Tauri 2.12.5, vários WebViews na mesma janela seguem atrás da feature
`unstable`. A janela própria sobre o painel dispensa essa feature.

## M1 — Consultar referências

- [ ] (padrão) Escolher a pasta de notas e lembrar da escolha
- [ ] (avançado) Definir uma CSP restritiva antes de renderizar conteúdo do usuário
- [ ] (padrão) Listar e abrir notas
- [ ] (padrão) Renderizar markdown com tabelas e imagens locais
- [ ] (padrão) Atalho global, ícone na bandeja, instância única e início com o Windows
- [ ] (avançado) Primeira definição visual, usando `tokens.css`
- [ ] (padrão) Criar `docs/DESIGN.md` com densidade, ícones, tom dos textos, estados e movimento
- [ ] (básico) Reprovar cor fixa fora de `tokens.css` no `check:fast`

**Pronto quando:** você consultar suas referências reais pelo painel no dia a dia.

## M2 — Criar e editar

- [ ] (padrão) Editor CodeMirror 6 com pré-visualização
- [ ] (avançado) Gravação atômica e autosave
- [ ] (avançado) Reação a alterações externas, sem perda de conteúdo
- [ ] (padrão) Colar imagens como anexos

## M3 — Encontrar

- [ ] (padrão) Índice SQLite FTS5 reconstruível
- [ ] (padrão) Busca rápida por teclado

## M4 — Páginas web

- [ ] (padrão) Salvar páginas como referências, usando a solução escolhida na M0

## M5 — Distribuir

- [ ] (padrão) Instalador, atualização automática e definição sobre assinatura de código

## Depois

Sem ordem definida:

- Captura de tela.
- Ditado com modelos locais, baixados à parte do instalador.
- Plugins: exigem uma decisão registrada antes de começar.
- macOS e Linux: nova implementação de `platform/`. No Wayland, o painel depende de layer-shell, que o
  GNOME não oferece.

## Questões em aberto

Valores padrão usados até você decidir o contrário.

| Questão                         | Padrão                                                                            |
| ------------------------------- | --------------------------------------------------------------------------------- |
| Lado da tela                    | Direito, configurável na M1                                                       |
| Monitor                         | Principal; suporte a mais monitores definido na M0                                |
| Fechar ao tirar o mouse         | Sim, exceto se fixado ou com edição ativa                                         |
| Página sobre apps em tela cheia | Funciona em janela e tela cheia sem bordas; jogos em modo exclusivo ficam de fora |
