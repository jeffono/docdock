# DocDock

[English](README.md) | Português (Brasil)

O DocDock é um painel lateral para Windows que se oculta automaticamente e mantém notas Markdown, imagens e páginas da web sempre à mão. Suas notas são arquivos `.md` comuns em uma pasta escolhida por você.

- Crie, edite e pesquise notas com salvamento automático; cole imagens nas notas.
- Fixe notas e páginas da web, navegue no painel e salve páginas como notas.
- Organize notas em pastas, mova ou renomeie e exclua para a Lixeira.
- Escolha o lado, a largura e o tema do painel; navegue com atalhos de teclado.

## Instalação

O DocDock requer Windows 10 ou 11 e Microsoft Edge WebView2. A primeira versão planejada, **0.1.0-beta.1**, é uma versão beta preliminar. Quando estiver disponível, baixe `DocDock_<version>_x64-setup.exe` em [GitHub Releases](https://github.com/jeffono/docdock/releases) e execute o instalador.

O instalador não tem assinatura de código. Se o Windows SmartScreen exibir “O Windows protegeu o computador”, selecione **Mais informações** e depois **Executar assim mesmo**. As atualizações automáticas estão planejadas, mas ainda não foram implementadas.

## Primeiros passos

Abra o DocDock e escolha uma pasta para suas notas. Mantenha o ponteiro na borda direita da tela para abrir o painel ou pressione **Ctrl+Alt+Espaço**. O menu da bandeja permite mudar o lado, iniciar o DocDock com o Windows ou sair. Use o alfinete do painel para mantê-lo aberto e a engrenagem para alterar as preferências. Pressione **Ctrl+/** para ver os atalhos.

## Privacidade

As notas são armazenadas localmente na pasta escolhida. O DocDock não tem telemetria. A navegação e as sugestões de pesquisa acessam os sites e o provedor de busca usados por você.

## Desenvolvimento

Instale Node 24, pnpm, Rust via rustup e as Ferramentas de Build C++ do Visual Studio. Em seguida, execute:

```sh
pnpm install
pnpm tauri dev
```

Consulte [AGENTS.md](AGENTS.md) para instruções de contribuição e a tabela de comandos. Execute `pnpm check` antes de integrar alterações.
