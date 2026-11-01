# DocDock — instruções para agentes

Painel lateral autohide para Windows: ao tocar a borda da tela com o mouse, ele se expande sobre as
outras janelas para consultar, adicionar e editar referências (markdown, imagens, páginas web).

Fonte única de instruções para Claude, Codex e Gemini. `CLAUDE.md` e `GEMINI.md` só importam este
arquivo. Ele é carregado em toda sessão: mantenha-o curto.

## Por onde começar

`docs/PLAN.md` diz o que fazer agora. Consulte `docs/ARCHITECTURE.md` e `docs/decisions/` só quando a
tarefa exigir. Não reabra decisões registradas sem um fato novo.

## Comandos

| Comando           | Para quê                                         |
| ----------------- | ------------------------------------------------ |
| `pnpm install`    | Instala dependências e ativa os hooks            |
| `pnpm tauri dev`  | Roda o app com hot reload da UI                  |
| `pnpm format`     | Formata TypeScript, Svelte e Rust                |
| `pnpm check:fast` | Lint, tipos e formatação; use durante o trabalho |
| `pnpm check`      | Verificação completa; use ao concluir a tarefa   |
| `pnpm test`       | Testes do frontend                               |

Use somente pnpm. Os hooks rodam `check:fast` no commit e `check` no push: nunca use `--no-verify`.

## Simplicidade

Resolva o problema de hoje com a solução mais simples que seja robusta. Sem abstração antes do segundo
uso real, sem configuração especulativa. Antes de adicionar, veja se dá para remover. Dependência nova
precisa de motivo claro na mensagem de commit.

## Arquitetura

- A UI só fala com o Rust por `src/lib/ipc.ts`.
- Win32 e outras APIs de sistema ficam em `src-tauri/src/platform/`.
- Separe decisão de efeito: regras são funções puras; a chamada ao sistema é uma camada fina.
- O conteúdo do usuário são arquivos `.md` comuns. Tudo o que for derivado precisa ser reconstruível.
- Nunca perca conteúdo: grave de forma atômica e não sobrescreva alterações externas sem avisar.
- Visual: use tokens e `src/lib/ui/`, nunca valores fixos. Só o usuário muda a identidade; crie token
  ou componente apenas se nada existente servir. Em UI, leia `docs/DESIGN.md` e entregue uma captura de
  tela para aprovação. Protótipos ficam de fora.
- Conteúdo renderizado (markdown, HTML, páginas web) é não confiável.

## Testes

Teste o que custa caro quando quebra: perda de conteúdo, máquina de estados, caminhos de arquivo e
segurança. Não teste repasses triviais, aparência ou o framework. Antes de criar um teste, responda
qual bug real ele pegaria. Correção de bug ganha teste de regressão, se o bug puder voltar. Se a suíte
passar de 30 testes ou 30 segundos, revise-a. Prefira rodar só o teste relevante durante o trabalho.

## Definição de pronto

- `pnpm check` passa.
- Comportamento visual (janela, foco, animação) tem roteiro de verificação manual na entrega.
- O que não pôde ser verificado é declarado na entrega.
- Os documentos afetados estão atualizados.

## Uso eficiente de cota

Cada tarefa do plano tem um nível; use o modelo mais barato que a resolva bem.

| Nível      | Uso                                                              |
| ---------- | ---------------------------------------------------------------- |
| `básico`   | Ajustes mecânicos, renomeações, texto, tarefa bem especificada   |
| `padrão`   | Funcionalidade com padrão já definido                            |
| `avançado` | Arquitetura, Win32, foco, concorrência, segurança, bugs difíceis |

- Leia só o necessário. Delegue apenas quando compensar: subagente começa sem contexto.
- Depois de duas tentativas sem sucesso, pare e suba de nível em vez de insistir.

## Contexto e documentação

Todo contexto durável fica no repositório. Memória privada de agente não é compartilhada.

| Informação                                  | Local                  |
| ------------------------------------------- | ---------------------- |
| Estado atual, próximo passo, tarefa aberta  | `docs/PLAN.md`         |
| Como o sistema funciona hoje                | `docs/ARCHITECTURE.md` |
| Por que uma decisão cara de reverter existe | `docs/decisions/`      |
| Detalhe de uma mudança                      | Mensagem de commit     |

- Ao pausar uma tarefa, deixe uma linha em "Em andamento" no plano; ao concluir, remova-a.
- Cada informação fica em um só lugar. Atualizar substitui o texto antigo.
- Etapa concluída vira uma linha. Sem changelog, diário ou README por pasta.

## Trabalho entre agentes

- Uma tarefa por branch: `tarefa/<assunto-curto>`. Em paralelo, use um worktree por agente em
  `../docdock-wt/<assunto>`.
- Confira `git status` e `git log -10` antes de começar e não mexa em mudanças alheias.
- Só faça merge em `main` quando o usuário pedir.
- Revisão por agente de outro fornecedor, só do diff: ao fechar cada etapa e antes de mudanças em
  persistência, segurança ou plugins. Protótipos ficam de fora.
- Ao fechar uma etapa, dispare o CI manual, se houver remoto.
- `pnpm tauri dev` usa a porta 1420 e o app ocupa a borda da tela: uma instância por vez na máquina.

## Decisões

Registre em `docs/decisions/` só o que for caro de reverter: formato de dados, segurança, plugins ou
biblioteca central. Use o próximo número e as seções Contexto, Decisão e Consequências.

Decida sozinho questões técnicas reversíveis. Pergunte ao usuário apenas sobre comportamento do
produto, aparência sem referência definida, custos e ações difíceis de desfazer.

## Idioma

Respostas, commits, PRs, branches, comentários no código e documentação em português do Brasil.
Nomes de variáveis, funções e tipos podem ficar em inglês.
