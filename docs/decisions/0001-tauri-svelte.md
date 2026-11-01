# 0001 — Tauri 2 com Svelte 5 e Vite

**Contexto:** o app precisa exibir markdown completo e páginas web, acessar APIs Win32 com facilidade
e, no futuro, rodar modelos locais de voz. O ciclo de desenvolvimento precisa ser rápido.

**Decisão:** usar Tauri 2 (Rust) com WebView2 e UI em Svelte 5 com Vite, sem SvelteKit: um painel sem
rotas não precisa de framework de aplicação.

**Consequências:**

- Instalador pequeno e acesso direto ao Win32 e a bibliotecas Rust de IA local.
- O uso de memória fica próximo ao de outros apps Chromium; a redução depende de medidas como baixar a
  prioridade de memória do WebView2 enquanto o painel estiver oculto.
- O motor web varia entre sistemas, e o Linux será a plataforma mais trabalhosa.
- Se a validação da etapa M0 falhar, a alternativa é o Electron. A UI pode ser reaproveitada.
