# DocDock

Painel lateral autohide para Windows, para consultar, adicionar e editar referências sem sair do que
você está fazendo.

Em desenvolvimento. Veja o andamento em [docs/PLAN.md](docs/PLAN.md).

## Rodar

Pré-requisitos: Node 24, pnpm, Rust (via rustup) e as Ferramentas de Build C++ do Visual Studio.

```sh
pnpm install
pnpm tauri dev
```

Antes de integrar uma mudança, rode `pnpm check`.

## Documentação

- [Plano](docs/PLAN.md)
- [Arquitetura](docs/ARCHITECTURE.md)
- [Decisões](docs/decisions/)
- [Instruções para agentes](AGENTS.md)
