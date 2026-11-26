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
| 4 | Sala e chat: protocolo, histórico, participantes | ✅ testado manualmente |
| 5 | Interface completa | ✅ testado manualmente |
| 6 | Robustez: reconexão, expulsão, encerramento | ✅ falta teste manual |
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
├── screens/          Telas (Home, CreateRoom, JoinRoom, Room)
├── components/       MessageList, MessageInput, ParticipantList, ConnectionBadge, PasswordMeter
├── hooks/useRoom.ts  Escuta os eventos de rede e expõe a sala + ações (criar, entrar, enviar, sair)
├── lib/tauri.ts      Única ponte com o núcleo Rust (invoke/listen tipados)
├── lib/room.ts       Estado da sala na interface e como cada evento o altera
├── lib/demo.ts       Sala de exemplo para o modo demonstração (?demo)
├── lib/password.ts   Estimativa de força da senha (medidor)
src-tauri/            Núcleo (Rust)
├── src/lib.rs        Monta o app e registra os comandos
├── src/commands.rs   Comandos chamados pela interface
├── src/protocol/     Mensagens (Hello, Pake*, Join, Chat...) e framing com limite de 64 KiB
├── src/room/         Convite e estado da sala (participantes, histórico, regras)
├── src/net/          iroh: endpoint, Host (retransmite), Convidado e handshake de senha
├── src/crypto/       Argon2id + SPAKE2, limite de tentativas, geração de senha
├── tauri.conf.json   Janela, CSP, empacotamento
└── capabilities/     Permissões da WebView (mínimas)
.github/workflows/    Compilação na nuvem (CI, build Windows, release)
```

## Teste manual

**Preparação:** baixe o `sala.exe` mais recente (veja "Para baixar o `.exe`" abaixo) nos computadores do teste.

1. **Mesmo computador (teste rápido):** abra o `sala.exe` três vezes.
   - Janela 1: **Criar sala** → apelido → **Gerar senha forte** → **Copiar senha** (guarde) → **Criar sala** → **Copiar convite**.
   - Janelas 2 e 3: **Entrar em uma sala** → convite, senha e apelido → **Entrar**.
   - Converse: cada mensagem deve aparecer nas três janelas, na mesma ordem. Enter envia; Shift+Enter quebra a linha.
   - Feche a janela 3 → as outras mostram "… saiu". Abra de novo e entre: ela recebe o histórico.
   - Entre com o mesmo apelido de alguém → ele vira "Apelido (2)".
   - Cole muitas mensagens seguidas bem rápido → aparece "Muitas mensagens seguidas. Espere um pouco."
   - Tente mandar `<b>oi</b>` → deve aparecer literalmente, sem negrito (texto nunca vira HTML).
   - Host: **Encerrar sala** (clique duas vezes para confirmar) → os Convidados veem "O Host encerrou a sala." e o campo de mensagem é desativado.
2. **Senha errada:** tente entrar com uma senha errada. O Convidado vê só "Não foi possível entrar na sala…" (sem dizer o motivo) e o Host vê "Tentativa de entrada recusada." na conversa. Depois de 5 erros, novas tentativas (até com a senha certa) são recusadas por alguns segundos, e a espera dobra a cada novo erro.
3. **Duas redes diferentes:** um computador na rede de casa e o outro no **4G do celular** (roteando a internet pelo celular). Repita os passos, mandando o convite por um app de mensagens e a senha por outro canal.
   - O selo deve mostrar **"Via relay"** no início e, em geral, mudar para **"Conexão direta"** em alguns segundos (hole punching).
   - Se ficar em "Via relay", também está certo: alguns tipos de NAT/CGNAT impedem a conexão direta. Os dados continuam cifrados.
   - O ping via relay costuma ser maior (dezenas a centenas de ms) que o direto.
4. **Sem internet (opcional):** marque **"Só rede local (sem relay)"** nos dois lados, com os dois computadores na mesma rede Wi-Fi/cabo.
5. **Queda e reconexão:** com um Convidado na sala, desligue o Wi-Fi dele por uns 10 segundos e ligue de novo.
   - O Convidado vê o selo **"Reconectando…"** e depois **"Reconectado."**, sem precisar colar convite nem senha.
   - Os outros veem "Fulano perdeu a conexão." (bolinha cinza, "offline") e depois "Fulano voltou.".
   - Mensagens enviadas enquanto ele estava fora aparecem quando volta.
   - Se ficar fora mais de 1 minuto, sai da sala; se a reconexão não der certo em 2 minutos, o Convidado vê uma mensagem explicando.
6. **Expulsar:** no Host, passe o mouse sobre um Convidado na lista → **remover** → **confirmar**. Ele vê "Você foi removido(a) da sala pelo Host." e os outros veem a saída.
7. **Fechar a janela:** feche o app do Host pelo X (sem clicar em Encerrar) → os Convidados veem "O Host encerrou a sala." (o app avisa antes de fechar). Feche um Convidado pelo X → os outros veem "… saiu." na hora.
8. **Convite inválido:** cole um convite cortado → deve aparecer uma mensagem de convite inválido.

### Regras da sala

| Regra | Valor |
|---|---|
| Apelido | 1 a 32 caracteres; repetido ganha número ("Ana (2)") |
| Mensagem | 1 a 4000 caracteres, exibida sempre como texto puro |
| Ritmo | rajada de até 10 mensagens; depois, 2 por segundo (o Host não tem limite) |
| Histórico | últimas 1000 mensagens, só na memória do Host; somem ao encerrar |
| Pessoas | até 50 por sala (contando o Host) |
| Ordem | o Host numera e carimba o horário de cada mensagem; todos veem a mesma ordem |
| Queda | quem cai fica "offline" por até 1 min e volta como a mesma pessoa (token de retomada) |
| Reconexão | o app do Convidado tenta sozinho por até 2 min (espera de 1 s, 2 s, 4 s… até 15 s) |
| Expulsar | o Host remove alguém; o token dessa pessoa deixa de valer |

**Limitação de expulsar:** quem foi removido mas sabe a senha consegue entrar de novo como um participante novo (a identidade de rede muda a cada conexão). Para afastar alguém de vez, encerre a sala e abra outra com outra senha.

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

Abra http://localhost:1420. Para ver a tela da sala com mensagens de exemplo, abra http://localhost:1420/?demo (o modo demonstração só existe fora do app).

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
