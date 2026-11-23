# Sala

Chat **peer-to-peer** privado para desktop (Windows, Linux, macOS), sem servidor central e sem banco de dados.

Uma pessoa (o **Host**) cria uma sala no próprio computador; outras pessoas (**Convidados**) entram usando um **convite + senha**. Todo o tráfego é criptografado de ponta a ponta, e as mensagens vivem só na memória do Host.

> A especificação completa está em [`SPEC.md`](SPEC.md).

## Status

| Fase | Descrição | Situação |
|---|---|---|
| 1 | Setup: Tauri 2 + React/TS/Vite/Tailwind, CI para os 3 sistemas | ✅ |
| 2 | Rede básica com iroh (Host ↔ Convidado, ping) | ✅ falta teste manual em redes diferentes |
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
├── screens/          Telas (Home, HostTest, GuestTest)
├── components/       ConnectionBadge (direta / via relay / desconectado)
├── lib/tauri.ts      Única ponte com o núcleo Rust (invoke/listen tipados)
src-tauri/            Núcleo (Rust)
├── src/lib.rs        Monta o app e registra os comandos
├── src/commands.rs   Comandos chamados pela interface
├── src/protocol/     Mensagens (Hello/Ping/Pong) e framing com prefixo de tamanho
├── src/room/         Convite (endereço do Host + id da sala)
├── src/net/          iroh: endpoint, Host (aceita) e Convidado (conecta)
├── tauri.conf.json   Janela, CSP, empacotamento
└── capabilities/     Permissões da WebView (mínimas)
.github/workflows/    Compilação na nuvem (CI, build Windows, release)
```

## Teste manual da conexão (fase 2)

Nesta fase ainda não há senha nem chat: só abrir a sala, conectar e medir a latência ("ping").

**Preparação:** baixe o `sala.exe` mais recente (veja "Para baixar o `.exe`" abaixo) nos dois computadores.

1. **Mesmo computador (teste rápido):** abra o `sala.exe` duas vezes. Na primeira, **Criar sala** → **Copiar convite**. Na segunda, **Entrar em uma sala** → cole o convite → **Conectar** → **Ping**. A janela do Host deve mostrar o Convidado em "Conectados".
2. **Duas redes diferentes (o teste que importa):** um computador na rede de casa e o outro no **4G do celular** (roteando a internet pelo celular). Repita os passos, mandando o convite por um app de mensagens.
   - O selo deve mostrar **"Via relay"** no início e, em geral, mudar para **"Conexão direta"** em alguns segundos (hole punching).
   - Se ficar em "Via relay", também está certo: alguns tipos de NAT/CGNAT impedem a conexão direta. Os dados continuam cifrados.
   - O ping via relay costuma ser maior (dezenas a centenas de ms) que o direto.
3. **Sem internet (opcional):** marque **"Só rede local (sem relay)"** nos dois lados, com os dois computadores na mesma rede Wi-Fi/cabo.
4. **Casos de erro:** feche a sala no Host → o Convidado deve mostrar "Desconectado". Cole um convite cortado → deve aparecer uma mensagem de convite inválido.

> O Windows pode pedir permissão de rede no firewall na primeira vez; permita para redes privadas.

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

**Metadados:** os relays do iroh conseguem ver *que* dois computadores estão trocando dados (e quando), mas não *o quê*. Ao conectar, os computadores trocam seus endereços IP para tentar a conexão direta, então os participantes de uma sala podem ver o IP uns dos outros.

**Convite:** no modo normal, o convite contém só a chave pública do Host, o servidor relay e o identificador da sala, **sem o IP do Host**. No modo "só rede local", ele contém os IPs locais. A chave pública do Host garante que o Convidado está falando com o Host verdadeiro: a conexão só abre se o Host provar que tem a chave privada correspondente.

**Regras do projeto:** sem telemetria; senhas, chaves e mensagens nunca vão para logs; mensagens são exibidas como texto puro (nunca HTML); a WebView tem CSP restritiva e permissões mínimas.

## Avisos ao rodar apps sem assinatura

Os instaladores ainda não são assinados digitalmente (isso exige certificados pagos):

- **Windows (SmartScreen):** "O Windows protegeu o computador" → clique em **Mais informações → Executar assim mesmo**.
- **macOS (Gatekeeper):** clique com o botão direito no app → **Abrir**, ou em *Ajustes do Sistema → Privacidade e Segurança → Abrir mesmo assim*.
- **Linux:** dê permissão de execução ao `.AppImage` (`chmod +x`).
