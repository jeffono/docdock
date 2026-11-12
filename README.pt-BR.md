# DocDock

[English](README.md) | Português (Brasil)

O DocDock é um painel lateral para Windows que se oculta automaticamente e mantém notas Markdown, imagens e páginas da web sempre à mão. Suas notas são arquivos `.md` comuns em uma pasta escolhida por você.

- Crie, edite e pesquise notas com salvamento automático; cole imagens nas notas.
- Fixe notas e páginas da web, navegue no painel e salve páginas como notas.
- Organize notas em pastas, mova ou renomeie e exclua para a Lixeira.
- Escolha o lado, a largura e o tema do painel; navegue com atalhos de teclado.

## Instalação

O DocDock requer Windows 10 ou 11. A primeira versão, **0.1.0-beta.1**, é uma beta preliminar: faça backup das suas notas antes de experimentá-la. Baixe `DocDock_<version>_x64-setup.exe` em [GitHub Releases](https://github.com/jeffono/docdock/releases) e execute o instalador. Ele instala para o usuário atual, sem exigir administrador, e baixa o Microsoft Edge WebView2 se estiver faltando.

O instalador não tem assinatura de código. Se o Windows SmartScreen exibir “O Windows protegeu o computador”, selecione **Mais informações** e depois **Executar assim mesmo**. O DocDock procura atualizações ao iniciar e as baixa em segundo plano; quando uma estiver pronta, escolha **Atualização disponível — reiniciar** no menu da bandeja. Sair normalmente não a instala.

## Primeiros passos

Abra o DocDock e escolha uma pasta para suas notas. Mantenha o ponteiro na borda direita da tela para abrir o painel ou pressione **Ctrl+Alt+Espaço**, que pode ser alterado em Preferências > Atalhos. O menu da bandeja permite mudar o lado, iniciar o DocDock com o Windows, instalar uma atualização ou sair. Use o alfinete do painel para mantê-lo aberto e a engrenagem para alterar as preferências. Pressione **Ctrl+/** para ver os atalhos.

## Privacidade

As notas são armazenadas localmente na pasta escolhida. O DocDock não tem telemetria. A cada inicialização, ele consulta o GitHub para saber se há uma versão nova. A navegação e as sugestões de pesquisa acessam os sites e o provedor de busca usados por você.

### Protegendo notas com Cryptomator

Um cofre opcional do [Cryptomator](https://cryptomator.org) criptografa as notas para sincronização na nuvem e ajuda a protegê-las se o dispositivo for roubado enquanto o cofre estiver bloqueado. O DocDock não armazena conteúdo das notas fora da pasta de notas.

1. No Cryptomator, crie um cofre dentro da pasta sincronizada com a nuvem e desbloqueie-o.
2. No DocDock, escolha uma pasta **dentro do cofre montado** como pasta de notas.
3. Opcionalmente, selecione o arquivo `vault.cryptomator` do cofre nas Preferências. A ação **Desbloquear** do painel abre esse arquivo no Cryptomator. O desbloqueio com Windows Hello é configurado no Cryptomator.

Enquanto o cofre está bloqueado, o DocDock mostra a pasta de notas como indisponível e nunca a recria. O texto não salvo fica na memória, com um aviso, e é salvo quando o cofre volta a ficar disponível; mantenha o DocDock aberto até o salvamento. Exclusões em um cofre sem Lixeira usam uma lixeira oculta dentro da pasta de notas, com a opção de desfazer enquanto o painel está aberto. O DocDock também solicita ao Windows que oculte o painel das capturas de tela (sem garantia em todos os casos) e mantém as cópias de notas fora do histórico e da sincronização da área de transferência.

**Mover notas existentes para um cofre não remove versões anteriores sem criptografia do histórico de versões ou da lixeira do provedor de nuvem, nem dos backups.** Apague também essas cópias anteriores ou comece com notas novas no cofre.

## Desenvolvimento

Instale Node 24, pnpm, Rust via rustup e as Ferramentas de Build C++ do Visual Studio. Em seguida, execute:

```sh
pnpm install
pnpm tauri dev
```

Consulte [AGENTS.md](AGENTS.md) para instruções de contribuição e a tabela de comandos. Execute `pnpm check` antes de integrar alterações.

## Licença

[MIT](LICENSE)
