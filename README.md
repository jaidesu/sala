# Sala

Chat **peer-to-peer** privado para desktop (Windows, Linux, macOS), sem servidor central e sem banco de dados.

Uma pessoa (o **Host**) cria uma sala no próprio computador; outras pessoas (**Convidados**) entram usando um **convite + senha**. Todo o tráfego é criptografado de ponta a ponta, e as mensagens vivem só na memória do Host.

> A especificação completa está em [`SPEC.md`](SPEC.md).

## Status

| Fase | Descrição | Situação |
|---|---|---|
| 1 | Setup: Tauri 2 + React/TS/Vite/Tailwind, CI para os 3 sistemas | ✅ |
| 2 | Rede básica com iroh (Host ↔ Convidado, ping) | ✅ testado em redes diferentes |
| 3 | Senha: Argon2id + SPAKE2, limite de tentativas | ✅ testado manualmente |
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
├── screens/          Telas (Home, CreateRoom, HostTest, GuestTest)
├── components/       ConnectionBadge, PasswordMeter
├── lib/tauri.ts      Única ponte com o núcleo Rust (invoke/listen tipados)
├── lib/password.ts   Estimativa de força da senha (medidor)
src-tauri/            Núcleo (Rust)
├── src/lib.rs        Monta o app e registra os comandos
├── src/commands.rs   Comandos chamados pela interface
├── src/protocol/     Mensagens (Hello, PakeMsg, PakeConfirm, Ping/Pong) e framing
├── src/room/         Convite (endereço do Host + id da sala)
├── src/net/          iroh: endpoint, Host, Convidado e handshake de senha (auth)
├── src/crypto/       Argon2id + SPAKE2, limite de tentativas, geração de senha
├── tauri.conf.json   Janela, CSP, empacotamento
└── capabilities/     Permissões da WebView (mínimas)
.github/workflows/    Compilação na nuvem (CI, build Windows, release)
```

## Teste manual (fases 2 e 3)

Ainda não há chat: só abrir a sala com senha, conectar e medir a latência ("ping").

**Preparação:** baixe o `sala.exe` mais recente (veja "Para baixar o `.exe`" abaixo) nos dois computadores.

1. **Mesmo computador (teste rápido):** abra o `sala.exe` duas vezes. Na primeira, **Criar sala** → **Gerar senha forte** (ou digite uma) → **Criar sala** → **Copiar convite**. Na segunda, **Entrar em uma sala** → cole o convite → digite a senha → **Conectar** → **Ping**. A janela do Host deve mostrar o Convidado em "Conectados". Criar e entrar levam ~1 s a mais que antes: é o Argon2id trabalhando.
2. **Senha errada:** tente entrar com uma senha errada. O Convidado vê só "Não foi possível entrar na sala…" (sem dizer o motivo) e o Host vê "tentativa de entrada recusada" na Atividade. Depois de 5 erros, novas tentativas (até com a senha certa) são recusadas por alguns segundos, e a espera dobra a cada novo erro.
3. **Duas redes diferentes:** um computador na rede de casa e o outro no **4G do celular** (roteando a internet pelo celular). Repita os passos, mandando o convite por um app de mensagens e a senha por outro canal.
   - O selo deve mostrar **"Via relay"** no início e, em geral, mudar para **"Conexão direta"** em alguns segundos (hole punching).
   - Se ficar em "Via relay", também está certo: alguns tipos de NAT/CGNAT impedem a conexão direta. Os dados continuam cifrados.
   - O ping via relay costuma ser maior (dezenas a centenas de ms) que o direto.
4. **Sem internet (opcional):** marque **"Só rede local (sem relay)"** nos dois lados, com os dois computadores na mesma rede Wi-Fi/cabo.
5. **Casos de erro:** feche a sala no Host → o Convidado deve mostrar "Desconectado". Cole um convite cortado → deve aparecer uma mensagem de convite inválido.

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

**Para baixar o `.exe`:** no GitHub, abra **Actions → Build Windows →** a execução mais recente → seção **Artifacts** → `sala-windows-portatil` (ou `-instalador`). Pelo terminal, dentro da pasta do projeto, troque `ID` pelo número da execução:

```bash
gh run download ID --name sala-windows-portatil --dir build-nuvem/ID
```

Cada execução vai para a própria pasta em `build-nuvem/`, que o git ignora. Assim uma versão nova não esbarra no `sala.exe` de uma versão anterior (o `gh` não sobrescreve arquivos existentes).

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

### Como a senha é verificada

A senha **nunca trafega pela rede** e quem grava o tráfego **não consegue testar senhas depois** (ataque de dicionário offline):

1. **Argon2id** (64 MiB, 3 passadas; sal = id da sala) transforma a senha em 32 bytes, de forma propositalmente lenta.
2. **SPAKE2** (crate `spake2`): cada lado envia uma mensagem de 33 bytes; os dois chegam à mesma chave só se a senha for a mesma. Cada palpite exige uma conversa nova com o Host.
3. **Amarração à conexão:** o handshake inclui as chaves públicas do Host e do Convidado, o id da sala e um valor exportado da sessão TLS (RFC 5705). Mensagens capturadas de uma conexão não servem em outra.
4. **Confirmação de chave (HMAC-SHA256):** o Convidado prova primeiro; o Host só responde se a prova estiver certa.
5. **Limite de tentativas no Host:** 5 erros livres; depois, bloqueio de 2 s que dobra a cada erro (até 5 min); no máximo 3 handshakes simultâneos; a contagem zera após 15 min sem erros. Em caso de falha, o Convidado recebe sempre a mesma mensagem genérica.

**Sem camada extra de cifragem:** o SPEC previa cifrar cada mensagem de novo com ChaCha20-Poly1305. Decidimos não fazer: o QUIC do iroh já cifra tudo com TLS 1.3 ponta a ponta, e a senha já é garantida pelos passos acima. Uma segunda camada seria código a mais, com risco a mais, sem proteção nova.

**Limitações conhecidas:**
- O limite de tentativas vale para a sala inteira (um atacante pode trocar de identidade à vontade). Então, durante um ataque, convidados legítimos também podem ter que esperar.
- A documentação do `spake2` avisa que ele provavelmente não roda em tempo constante. Pela rede, a variação de tempo é muito menor que a latência, e o limite de tentativas restringe o número de medições. Fica registrado.
- O medidor de força é uma estimativa simples. Senhas escolhidas por pessoas costumam ser mais fracas do que parecem; prefira a senha gerada (16 caracteres aleatórios, ~79 bits).

**Regras do projeto:** sem telemetria; senhas, chaves e mensagens nunca vão para logs; mensagens são exibidas como texto puro (nunca HTML); a WebView tem CSP restritiva e permissões mínimas.

## Avisos ao rodar apps sem assinatura

Os instaladores ainda não são assinados digitalmente (isso exige certificados pagos):

- **Windows (SmartScreen):** "O Windows protegeu o computador" → clique em **Mais informações → Executar assim mesmo**.
- **macOS (Gatekeeper):** clique com o botão direito no app → **Abrir**, ou em *Ajustes do Sistema → Privacidade e Segurança → Abrir mesmo assim*.
- **Linux:** dê permissão de execução ao `.AppImage` (`chmod +x`).
