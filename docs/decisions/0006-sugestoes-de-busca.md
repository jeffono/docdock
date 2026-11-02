# 0006 — Sugestões de busca na web

**Contexto:** a barra única serve para buscar notas, criar notas e abrir páginas. Sugestões de um
buscador melhoram a navegação, mas enviar cada tecla mandaria para fora o que se procura nas notas,
que são privadas.

**Decisão:**

- **Nível 1, ligado por padrão:** consulta o buscador só com intenção clara de web: no modo navegador, ou
  nas notas depois de escolher "Buscar na web" ou usar o prefixo `?`. O que se digita para buscar notas
  nunca sai do computador.
- **Nível 2, opção desligada por padrão:** consulta a cada tecla também ao buscar notas, com espera curta
  e mínimo de letras.
- **Buscador:** DuckDuckGo por padrão, trocável nas configurações. O Google não tem endereço oficial de
  sugestões.
- **Pedido pelo Rust:** sem cookies e sem referer. A resposta é JSON não confiável, mostrada como texto,
  nunca como HTML. A CSP da janela `main` não muda.
- **Histórico de endereços:** local, em `app_config_dir`, nunca na pasta de notas.

**Consequências:**

- Entra um cliente HTTP leve no Rust.
- O buscador recebe o texto e o IP; offline ou fora do tempo limite, só aparecem sugestões locais.
- Respostas atrasadas são descartadas para não sobrescrever a digitação atual.
