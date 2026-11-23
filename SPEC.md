# Chat P2P Privado — Especificação do Projeto

> Documento para o Claude Code iniciar o desenvolvimento. Nome provisório do app: **Sala** (pode ser trocado).

## 1. Visão geral

Aplicativo desktop multiplataforma (Windows, Linux, macOS) de chat **peer-to-peer, sem servidor central e sem banco de dados**.

- Uma pessoa (**Host**) cria uma sala no próprio computador.
- Outras pessoas (**Convidados**) entram na sala a partir dos próprios computadores, usando um **convite + senha**.
- Somente quem tem o convite e a senha consegue entrar. Todo o tráfego é criptografado de ponta a ponta.
- O computador do Host é a "fonte da verdade": as mensagens passam por ele e vivem na memória dele. Não existe nenhum servidor de terceiros que armazene conteúdo.

### Princípios

1. **Privacidade primeiro**: nenhum dado de conteúdo sai dos computadores dos participantes.
2. **Zero configuração de rede para o usuário**: sem abrir portas no roteador, sem IP fixo.
3. **Instalação fácil**: um instalador/binário por sistema operacional, sem dependências externas.
4. **Efêmero por padrão**: mensagens ficam apenas em memória; persistência é opcional e criptografada.

## 2. Requisitos funcionais

### MVP
- Criar sala (Host): gera convite e define a senha (ou gera uma senha forte automaticamente).
- Entrar na sala (Convidado): colar o convite, digitar a senha, entrar.
- Enviar e receber mensagens de texto em tempo real entre todos os participantes.
- Lista de participantes com apelido (nickname) e status online/offline.
- Notificação de entrada/saída de participantes.
- Histórico enviado ao Convidado quando entra (do que estiver na memória do Host).
- Host pode expulsar participantes e encerrar a sala.
- Indicador de estado da conexão (conectando / direta / via relay / desconectado).
- Senha incorreta: mensagem genérica de falha, sem revelar detalhes.

### Fase 2
- Persistência local opcional do histórico, criptografada em disco.
- Envio de arquivos/imagens (chunked, com limite de tamanho configurável).
- Indicador "digitando...", confirmação de entrega.
- Notificações nativas do sistema operacional.
- Tema claro/escuro.

### Fora de escopo (por enquanto)
- Contas de usuário, login, cadastro.
- Chamadas de voz/vídeo.
- Aplicativo móvel.
- Salas que continuam existindo com o Host offline.

## 3. Decisão de tecnologia

### Stack escolhida

| Camada | Tecnologia | Motivo |
|---|---|---|
| Shell desktop | **Tauri 2** | Gera instaladores leves para Windows/Linux/macOS; usa a WebView do sistema (binário de poucos MB, ao contrário do Electron); backend em Rust. |
| Núcleo (rede + cripto + estado) | **Rust** | Segurança de memória, ótimas bibliotecas de criptografia e rede, roda dentro do Tauri como backend. |
| Rede P2P | **iroh** (crate Rust) | Conexões QUIC diretas entre dispositivos, com *hole punching* automático (NAT traversal) e fallback por *relays* que só enxergam tráfego criptografado. Resolve o maior problema de apps P2P: alcançar o Host atrás de roteador/NAT sem configurar nada. Cada nó tem uma identidade (chave pública) e a conexão já é autenticada e criptografada (TLS 1.3 sobre QUIC). |
| Autenticação por senha | **PAKE** (SPAKE2 ou CPace) + **Argon2id** | A senha nunca trafega pela rede e não pode ser quebrada offline por quem observa a conexão. |
| Interface | **React + TypeScript + Vite** | Stack que o desenvolvedor já domina. Estilo: Tailwind CSS. |
| Comunicação UI ↔ núcleo | **Comandos e eventos do Tauri** (`invoke` / `emit`) | A UI nunca toca na rede diretamente; só conversa com o núcleo Rust. |
| Serialização de mensagens | JSON com prefixo de tamanho (MVP); `postcard` opcional depois | Simples de depurar. |
| Empacotamento | `tauri build` + GitHub Actions (matriz win/linux/mac) | Gera `.msi`/`.exe`, `.dmg`/`.app`, `.AppImage`/`.deb` automaticamente. |

> **Instrução ao Claude Code**: antes de começar, verifique as versões atuais e a API vigente de `iroh`, `tauri` e do crate PAKE escolhido (a API do iroh muda entre versões). Ajuste o código à documentação oficial, não à memória.

### Alternativa (plano B)
**Electron + Hyperswarm (Node.js/TypeScript)**: tudo em JS/TS, descoberta de par por tópico derivado de hash (via DHT) e hole punching embutido. É mais rápido de prototipar, porém o instalador é bem mais pesado e a gestão de segurança fica mais manual. Usar somente se a integração com o iroh se mostrar inviável.

### Por que não...
- **WebRTC puro**: exige servidor de sinalização (mesmo que mínimo) e servidores STUN/TURN; vai contra a proposta de não depender de infraestrutura.
- **Conexão TCP direta / abrir porta no roteador**: inviável para usuários comuns (CGNAT, firewall, UPnP desligado).
- **Tor (onion services)**: ótimo anonimato, mas latência alta e a biblioteca Rust de onion services hospedados ainda é menos madura; pode ser considerado como modo opcional no futuro.
- **Banco de dados (SQLite etc.)**: desnecessário no MVP. Estado vive em memória do Host. Persistência opcional na fase 2 usa arquivo local criptografado.

## 4. Arquitetura

### Topologia: estrela com o Host no centro

```
 Convidado A ──┐
                ├──► HOST (sala, histórico em memória, autoridade)
 Convidado B ──┘
```

- Convidados conectam **apenas ao Host** (não entre si). O Host valida, ordena e retransmite as mensagens a todos.
- Vantagens: ordenação consistente, controle de acesso simples, expulsão trivial, menos conexões.
- Limitação aceita: se o Host fica offline, a sala acaba (a sala é efêmera).

### Módulos do núcleo Rust (`src-tauri/src/`)

```
src-tauri/src/
├── main.rs            # bootstrap Tauri, registra comandos
├── commands.rs        # comandos expostos à UI (create_room, join_room, send_message, kick, leave...)
├── net/
│   ├── endpoint.rs    # criação/gestão do endpoint iroh
│   ├── host.rs        # aceita conexões, roda a sala
│   └── guest.rs       # conecta ao host
├── crypto/
│   ├── pake.rs        # handshake SPAKE2/CPace + Argon2id
│   └── session.rs     # chaves de sessão, cifragem das mensagens da aplicação
├── protocol/
│   ├── messages.rs    # enums/structs de mensagens do protocolo
│   └── framing.rs     # prefixo de tamanho, limites
├── room/
│   ├── state.rs       # participantes, histórico em memória, regras
│   └── invite.rs      # geração/parse do convite
└── store/             # (fase 2) persistência criptografada opcional
```

### Frontend (`src/`)

```
src/
├── App.tsx
├── screens/           # Home, CreateRoom, JoinRoom, Room
├── components/        # MessageList, MessageInput, ParticipantList, ConnectionBadge
├── hooks/             # useRoom (escuta eventos do Tauri)
└── lib/tauri.ts       # wrappers tipados de invoke/listen
```

## 5. Fluxos

### 5.1 Criar sala (Host)
1. Usuário informa apelido e (opcional) nome da sala; define a senha ou clica em "gerar senha forte".
2. O núcleo cria um endpoint iroh e obtém o identificador do nó (chave pública + info de relay).
3. O app gera o **convite**: string curta, copiável, contendo o endereço do Host (NodeId/ticket do iroh) e um identificador da sala. **A senha não faz parte do convite**; é compartilhada separadamente pelo usuário.
4. A sala fica aberta enquanto o app do Host estiver rodando.

### 5.2 Entrar na sala (Convidado)
1. Usuário cola o convite, digita a senha e o apelido.
2. O núcleo conecta ao Host via iroh (direto, ou via relay se o hole punching falhar).
3. Executa o **handshake PAKE** (ver seção 6). Se falhar, a conexão é fechada com resposta genérica.
4. Se passar, o Host envia: lista de participantes + histórico em memória; avisa aos demais que o Convidado entrou.

### 5.3 Troca de mensagens
1. Convidado envia `ChatMessage` ao Host.
2. Host valida (tamanho, taxa), carimba `id` e `timestamp` do Host, adiciona ao histórico em memória.
3. Host retransmite a todos os participantes (inclusive o remetente, como confirmação).

## 6. Segurança (parte crítica)

### Modelo de ameaça
- Protege contra: bisbilhoteiro na rede, operadores dos relays, quem descobrir o convite mas não tem a senha, tentativas de adivinhar a senha.
- **Não** protege contra: o próprio Host (ele vê tudo, por design), participante que copia mensagens, malware no computador de alguém.
- Metadados: relays do iroh podem ver que há tráfego entre dois nós (mas não o conteúdo). Documentar isso na UI/README. Futuramente permitir relay próprio configurável.

### Camadas
1. **Transporte**: iroh/QUIC já fornece criptografia TLS 1.3 e autentica o Host pela chave pública presente no convite (impede impostor do Host).
2. **Autenticação por senha (PAKE)**: após conectar, Host e Convidado rodam SPAKE2 (ou CPace) usando a senha **previamente esticada com Argon2id** (sal derivado do identificador da sala). Resultado: chave de sessão compartilhada sem a senha trafegar e sem permitir ataque de dicionário offline a partir de uma captura.
   - Vincular o handshake ao canal (incluir NodeIds de ambos e o identificador da sala no transcript do PAKE).
3. **Criptografia da aplicação**: mensagens de chat cifradas com **ChaCha20-Poly1305** (ou AES-256-GCM) usando chaves derivadas da chave de sessão via HKDF; nonce por contador, direção separada (chaves distintas Host→Convidado e Convidado→Host).
4. **Proteção contra força bruta**: o Host limita tentativas de senha por conexão e por janela de tempo (backoff exponencial, bloqueio temporário), e fecha a conexão após N falhas. Nunca informar *por que* falhou.
5. **Senha forte**: sugerir/gerar passphrase (ex.: 5–6 palavras aleatórias) e mostrar medidor de força; avisar quando fraca.
6. **Higiene**: usar a crate `zeroize` para limpar senha/chaves da memória; não registrar (log) senhas, chaves ou conteúdo de mensagens; nunca enviar telemetria.
7. **Tauri**: CSP restritiva, capabilities mínimas, sem acesso arbitrário a sistema de arquivos pela WebView; sanitizar todo texto renderizado (evitar XSS — renderizar mensagens como texto puro, nunca como HTML).

## 7. Protocolo de mensagens (rascunho)

Todas as mensagens têm quadro com prefixo de tamanho, e limite máximo (ex.: 64 KB para texto no MVP).

```
// Antes do PAKE (apenas handshake)
Hello        { version, nickname_hint? }
PakeMsg      { bytes }

// Depois do PAKE (tudo cifrado)
Joined       { participants: [Participant], history: [ChatMessage] }
ParticipantJoined { participant }
ParticipantLeft   { participant_id }
ChatMessage  { id, from, text, timestamp }
SendMessage  { text }                 // Convidado -> Host
Kick         { participant_id }       // Host -> alvo
RoomClosed   { }                      // Host -> todos
Ping / Pong
```

- Versionar o protocolo (`version`) desde o início.
- Limitar tamanho de apelido (ex.: 32 chars), de mensagem e a taxa de envio por participante.

## 8. Persistência

- **MVP**: nada em disco. Histórico limitado em memória (ex.: últimas 1000 mensagens), descartado ao fechar a sala.
- **Fase 2 (opcional)**: histórico salvo em arquivo local criptografado (chave derivada via Argon2id de uma senha local ou da senha da sala). Sem SQLite no começo; um arquivo cifrado append-only basta.

## 9. UX / Telas

1. **Home**: botões "Criar sala" e "Entrar em uma sala".
2. **Criar sala**: apelido, senha (campo + gerar), botão criar → mostra convite com botão "copiar" e lembrete de enviar a senha por outro canal.
3. **Entrar**: campo de convite, senha, apelido.
4. **Sala**: lista de mensagens, campo de entrada, painel de participantes, badge de conexão, botão sair (Host: encerrar sala).
- Visual limpo e minimalista, tema escuro por padrão, atalhos (Enter envia, Shift+Enter quebra linha).

## 10. Plano de implementação (fases)

1. **Setup**: projeto Tauri 2 + React/TS/Vite/Tailwind; CI de build para os 3 SOs.
2. **Rede básica**: Host abre endpoint iroh, Convidado conecta pelo convite, troca de "ping" cifrado pelo QUIC. Testar em duas máquinas em redes diferentes.
3. **Senha/PAKE**: handshake com Argon2id + SPAKE2, rejeição de senha errada, rate limiting.
4. **Sala e chat**: protocolo, estado da sala, retransmissão, histórico, participantes.
5. **UI**: telas e integração por eventos Tauri.
6. **Robustez**: reconexão, quedas de rede, expulsão, encerramento limpo, tratamento de erros amigável.
7. **Empacotamento e release**: instaladores para Windows, macOS e Linux via GitHub Actions.
8. **Fase 2**: persistência cifrada, arquivos, notificações.

## 11. Testes

- Unitários (Rust): PAKE com senha certa/errada, cifragem/decifragem, parsing de convite, limites do protocolo.
- Integração: Host e N Convidados em processos locais; cenários de senha errada, desconexão abrupta, expulsão.
- Manual: duas máquinas em redes diferentes (uma delas em rede móvel/CGNAT) para validar NAT traversal.

## 12. Riscos e pontos de atenção

- **NAT traversal nem sempre faz conexão direta**: nesses casos o tráfego passa pelo relay (cifrado). Mostrar isso ao usuário.
- **Host precisa estar online** e o computador não pode suspender; avisar na UI.
- **Assinatura de código**: macOS (notarização) e Windows (SmartScreen) exibem alertas para apps sem assinatura; documentar no README como contornar durante o desenvolvimento.
- **Mudanças de API** do iroh: fixar versões no `Cargo.lock` e consultar a documentação oficial.
- **Não inventar criptografia**: usar apenas primitivas e bibliotecas auditadas; qualquer dúvida sobre o handshake deve ser sinalizada ao desenvolvedor em vez de improvisada.

## 13. Instruções para o Claude Code

1. Leia este documento inteiro antes de escrever código.
2. Confirme as versões atuais e a documentação de Tauri 2, iroh e da crate PAKE antes de começar.
3. Siga as fases da seção 10, em ordem. Ao fim de cada fase, rode os testes e me mostre como validar manualmente.
4. Se algo neste documento for inviável ou desatualizado, proponha uma alternativa e justifique, antes de implementar.
5. Comente o código de forma clara; o desenvolvedor é estudante e quer aprender com o projeto.
6. Mantenha um `README.md` com instruções de build, execução e o modelo de ameaça resumido.
