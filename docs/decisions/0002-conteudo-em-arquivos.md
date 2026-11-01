# 0002 — Conteúdo em arquivos markdown comuns

**Contexto:** referências são dados pessoais de longo prazo e não devem ficar presas ao app.

**Decisão:** guardar o conteúdo como arquivos `.md` e anexos em uma pasta do usuário. Qualquer banco de
dados é apenas um índice descartável.

**Consequências:**

- O conteúdo continua legível sem o app e pode ser versionado ou sincronizado por outras ferramentas.
- O app precisa lidar com alterações externas e com gravação atômica.
- Recursos que dependam de metadados usam frontmatter no próprio arquivo.
