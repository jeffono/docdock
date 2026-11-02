# 0005 — Itens fixados em `_pinned.md`

**Contexto:** o painel vai mostrar notas e páginas web fixadas no topo, em ordem escolhida pelo usuário.
A lista precisa acompanhar a pasta entre computadores, funcionar fora do app e não tocar nas notas a
cada clique.

**Decisão:**

- **Arquivo:** `_pinned.md` na raiz da pasta de notas. O `_` o mantém no topo do Explorer, e ele aparece
  na lista do app como qualquer nota.
- **Formato:** markdown comum. Cada item de lista com um link é um item fixado, na ordem do arquivo:
  link relativo para nota, com espaço como `%20`, ou URL http(s) para página. Sem `[[wikilinks]]`, que só
  o Obsidian entende.
- **Edição por linha:** fixar acrescenta uma linha, desafixar remove a linha daquele link e reordenar move
  linhas. O app nunca regrava o arquivo inteiro, então títulos e comentários escritos à mão ficam.
- **Estado do acordeão:** quais itens estão recolhidos fica em `settings.json`, por computador.
- **Restauração:** o app guarda a última versão lida em `app_config_dir`. Se `_pinned.md` sumir, oferece
  "Restaurar fixadas"; ao fixar algo sem restaurar, cria o arquivo de novo.

**Consequências:**

- Nota renomeada ou movida fora do app quebra o link; o item aparece como "nota não encontrada", com a
  opção de removê-lo.
- Edições externas em `_pinned.md` chegam pela reação a alterações externas da M2.
- O arquivo é conteúdo do usuário: vale a gravação atômica da M2.
