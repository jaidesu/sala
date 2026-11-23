# Sala

Chat **peer-to-peer** privado para desktop (Windows, Linux, macOS), sem servidor central e sem banco de dados.

Uma pessoa (o **Host**) cria uma sala no próprio computador; outras pessoas (**Convidados**) entram usando um **convite + senha**. Todo o tráfego é criptografado de ponta a ponta, e as mensagens vivem só na memória do Host.

> A especificação completa está em [`SPEC.md`](SPEC.md).

## Status

| Fase | Descrição | Situação |
|---|---|---|
| 1 | Setup: Tauri 2 + React/TS/Vite/Tailwind, CI para os 3 sistemas | ✅ em andamento |
| 2 | Rede básica com iroh (Host ↔ Convidado) | ⏳ |
| 3 | Senha: Argon2id + SPAKE2, limite de tentativas | ⏳ |
| 4 | Sala e chat: protocolo, histórico, participantes | ⏳ |
| 5 | Interface completa | ⏳ |
| 6 | Robustez: reconexão, expulsão, encerramento | ⏳ |
| 7 | Instaladores e releases | ⏳ |

## Stack

| Camada | Tecnologia |
|---|---|
| App desktop | [Tauri 2](https://tauri.app) (usa a WebView do sistema; binário pequeno) |
| Núcleo (rede, cripto, estado) | Rust |
| Rede P2P | [iroh](https://iroh.computer) (QUIC com *hole punching* e relays cifrados) |
| Senha | Argon2id + SPAKE2 (a senha nunca trafega na rede) |
| Interface | React + TypeScript + Vite + Tailwind CSS v4 |

## Estrutura

```
src/                  Interface (React)
├── screens/          Telas (Home, ...)
├── lib/tauri.ts      Única ponte com o núcleo Rust (invoke/listen tipados)
src-tauri/            Núcleo (Rust)
├── src/lib.rs        Monta o app e registra os comandos
├── src/commands.rs   Comandos chamados pela interface
├── src/protocol/     Protocolo de mensagens (versão, tipos, framing)
├── tauri.conf.json   Janela, CSP, empacotamento
└── capabilities/     Permissões da WebView (mínimas)
.github/workflows/    Compilação na nuvem (CI, build Windows, release)
```

> Observação: o SPEC.md cita `main.rs` registrando os comandos. No Tauri 2 o padrão é o `main.rs` só chamar `sala_lib::run()`, e o registro ficar em `lib.rs`. A ideia é a mesma.

## Como compilar

### Na nuvem (padrão deste projeto)

O computador de desenvolvimento não tem o compilador Rust; quem compila é o **GitHub Actions**:

| Workflow | Quando roda | O que faz |
|---|---|---|
| `CI` | todo envio para `main` e pull requests | compila a interface; `cargo fmt`, `clippy` e `cargo test` no Linux |
| `Build Windows` | todo envio para `main` ou manual | gera o instalador `.exe` e o executável portátil |
| `Release` | ao enviar uma tag `v*` (ex.: `v0.1.0`) | gera instaladores para Windows, Linux e macOS num rascunho de Release |

**Para baixar o `.exe`:** no GitHub, abra **Actions → Build Windows →** a execução mais recente → seção **Artifacts** → `sala-windows-portatil` (ou `-instalador`). Pelo terminal:

```bash
gh run download --name sala-windows-portatil
```

### Só a interface, sem Rust

Dá para ajustar o visual no navegador, sem compilar o núcleo (os botões que dependem do Rust ficam inativos):

```bash
npm install
npm run dev
```

Abra http://localhost:1420.

### Localmente, com Rust (opcional)

Requer [Rust](https://rustup.rs), Node.js e os [pré-requisitos do Tauri](https://tauri.app/start/prerequisites/) (no Windows: Visual Studio Build Tools com C++ e WebView2).

```bash
npm install
npm run tauri dev
```

## Segurança: modelo de ameaça (resumo)

**Protege contra:**
- quem observa a rede (Wi-Fi, provedor);
- os operadores dos relays do iroh (só veem tráfego cifrado);
- quem descobre o convite mas não sabe a senha;
- tentativas de adivinhar a senha (limite de tentativas, sem ataque de dicionário offline).

**Não protege contra:**
- o próprio Host (ele vê todas as mensagens, por design);
- um participante que copia ou fotografa as mensagens;
- malware no computador de alguém.

**Metadados:** os relays do iroh conseguem ver *que* dois computadores estão trocando dados (e quando), mas não *o quê*.

**Regras do projeto:** sem telemetria; senhas, chaves e mensagens nunca vão para logs; mensagens são exibidas como texto puro (nunca HTML); a WebView tem CSP restritiva e permissões mínimas.

## Avisos ao rodar apps sem assinatura

Os instaladores ainda não são assinados digitalmente (isso exige certificados pagos):

- **Windows (SmartScreen):** "O Windows protegeu o computador" → clique em **Mais informações → Executar assim mesmo**.
- **macOS (Gatekeeper):** clique com o botão direito no app → **Abrir**, ou em *Ajustes do Sistema → Privacidade e Segurança → Abrir mesmo assim*.
- **Linux:** dê permissão de execução ao `.AppImage` (`chmod +x`).
