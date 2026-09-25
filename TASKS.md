### US01: Arquitetura Multi-Tenant e Banco de Dados
**Épico:** Fundação SaaS | **Componente:** Cloud API (Rust/SQLx) | **Risco:** Alto (OWASP A01 - Quebra de Isolamento)

**Contexto de Negócio:**
Esta é a fundação do nosso SaaS. O sistema atenderá centenas de lojistas no mesmo banco de dados PostgreSQL. O objetivo desta história não é apenas criar as tabelas, mas garantir arquitetonicamente no código Rust que é impossível um desenvolvedor esquecer de passar o `store_id` em uma query, prevenindo o vazamento de dados entre empresas. E para garantir a precisão dos relatórios financeiros, devemos congelar todos os valores do momento da venda.

#### Task 1.1: Criação das Migrations Iniciais (SQLx)
**Descrição:** Criar os arquivos `.sql` de migração estrutural no backend utilizando a CLI do SQLx, refletindo exatamente o diagrama relacional acordado, com ênfase nas chaves estrangeiras, isolamento Multi-Tenant e Snapshots Históricos.
**Critérios de Aceite (DoD):**
*   [ ] O arquivo de migração `001_initial_schema.sql` deve conter a criação de todas as tabelas principais (`STORE_SETTINGS`, `USER`, `PRODUCT`, `CUSTOMER`, `SALE`, `SALE_ITEM`, `STOCK`, `PAYMENT`).
*   [ ] Todas as tabelas operacionais devem possuir a coluna `store_id` (tipo `UUID`) configurada como `FOREIGN KEY` referenciando `STORE_SETTINGS(id)` com `ON DELETE RESTRICT`.
*   [ ] Tabelas como `PRODUCT` e `USER` devem possuir a coluna `is_active` (`BOOLEAN DEFAULT TRUE`) para suportar Soft Delete.
*   [ ] A tabela `SALE_ITEM` deve conter os campos do Tax Snapshot (`ncm_code`, `cfop`, `icms_rate`, etc.) com tipagem rigorosa (`VARCHAR`, `DECIMAL`).
*   [ ] **[ATUALIZAÇÃO DE REGRA]:** A tabela `SALE_ITEM` deve incluir a coluna `unit_cost_price` (Decimal) para congelar o custo de aquisição da mercadoria no momento exato da venda, garantindo que o cálculo de lucro bruto futuro não seja corrompido.
*   [ ] O comando `cargo sqlx migrate run` deve executar sem erros em um banco PostgreSQL limpo.
**Cenários de Teste (QA & Banco):**
*   **Happy Path:** Executar a migração, conectar ao banco via DBeaver/PgAdmin e confirmar a criação da estrutura.
*   **Edge Case (Integridade Restrita):** Tentar excluir uma loja manualmente no banco (`DELETE FROM STORE_SETTINGS`). O banco deve bloquear a operação se existirem vendas vinculadas, validando o `ON DELETE RESTRICT`.

#### Task 1.2: Modelagem de Structs e Traits de Repositório (Rust)
**Descrição:** Mapear as tabelas do PostgreSQL para estruturas (Structs) no Rust utilizando as macros do SQLx, e criar a interface (Trait) de comunicação com o banco exigindo o `store_id` como parâmetro obrigatório em qualquer função.
**Critérios de Aceite (DoD):**
*   [ ] Criar os arquivos dentro de `cloud-api/src/models/` com as Structs derivando `Serialize`, `Deserialize` e `FromRow` (SQLx).
*   [ ] Utilizar tipos adequados do Rust (ex: `uuid::Uuid` para chaves, `rust_decimal::Decimal` para valores monetários e taxas, `chrono::NaiveDateTime` para datas).
*   [ ] Criar os repositórios base em `cloud-api/src/repositories/` contendo as funções de CRUD.
*   [ ] Nenhuma assinatura de função no repositório que faça `SELECT`, `UPDATE` ou `DELETE` pode existir sem receber `store_id: Uuid` como argumento obrigatório.
**Cenários de Teste (Code Review & Unit):**
*   **Happy Path:** As structs compilam corretamente e os tipos batem com o banco (verificado via `cargo sqlx prepare`).
*   **Security Case (Prevenção de Injeção):** Verificar via Code Review se todas as queries estão utilizando prepared statements (macro `query!` ou `query_as!`) e não concatenação de strings com `format!()`.

#### Task 1.3: Middleware Axum e Extrator de Contexto (Isolamento Lógico)
**Descrição:** Como a autenticação completa com JWT virá na US02, precisamos preparar o "Extractor" do Axum nesta US. Ele será responsável por extrair o `store_id` do cabeçalho da requisição (futuramente do JWT) e injetá-lo de forma limpa em todas as rotas (Controllers).
**Critérios de Aceite (DoD):**
*   [ ] Criar um custom extractor no Axum chamado `TenantContext` que implemente a trait `FromRequestParts`.
*   [ ] Para esta US, o extractor deve ler um header temporário (ex: `X-Store-ID`) e validar se é um UUID válido. (Na US02, isso será substituído pela leitura do payload do JWT).
*   [ ] Criar uma rota de teste `GET /api/v1/ping` que receba o `TenantContext` e retorne um JSON confirmando o `store_id` identificado.
*   [ ] Se o header não for enviado ou for inválido, o extractor deve rejeitar a requisição imediatamente com HTTP 400 Bad Request.
**Cenários de Teste (API & Segurança):**
*   **Happy Path:** Fazer um `GET /api/v1/ping` enviando o header `X-Store-ID: <uuid-valido>`. Receber HTTP 200.
*   **Security Case (Falta de Contexto):** Fazer a mesma requisição sem o header. A requisição nem deve chegar no handler do controller, sendo barrada pelo Axum com HTTP 400.

---

### US02: Autenticação e Perfis (Gestão de Usuários)
**Épico:** Fundação SaaS | **Componentes:** Cloud API (Rust) e Desktop (Tauri/React) | **Risco:** Alto (OWASP A02 e A07 - Falhas de Criptografia e Autenticação)

**Contexto de Negócio:**
O controle de quem opera o caixa e de quem gerencia a loja é a base da segurança antifraude do sistema. O caixa precisa de um acesso rápido e restrito à tela de vendas, enquanto o gerente precisa acessar relatórios e autorizar exceções. A emissão do token JWT será o "crachá" digital.

#### Task 2.1: Cadastro de Usuários e Hashing Criptográfico (Backend)
**Descrição:** Implementar a lógica de criação de usuários na tabela `USER`, garantindo que senhas/PINs nunca transitem de forma insegura.
**Critérios de Aceite (DoD):**
*   [ ] O payload de criação deve exigir o `store_id` implícito do criador (para garantir que um gerente só crie usuários para a sua própria loja).
*   [ ] A senha recebida deve ser imediatamente processada via hashing robusto com salt (ex: Argon2) antes de qualquer persistência no PostgreSQL.
*   [ ] O campo de papel (`role`) deve ser restrito a uma lista pré-definida e tipada (ex: `MANAGER`, `CASHIER`, `ADMIN`), rejeitando qualquer outro valor.
**Cenários de Teste (QA & Segurança):**
*   **Security Case (Proteção de Credencial):** Inspecionar o banco diretamente após a criação. O campo `pin_hash` deve ser irreversível (hash criptográfico).
*   **Security Case (Isolamento de Tenant):** Um gerente da Loja A tenta enviar payload forçando a criação para a Loja B. O backend deve ignorar o ID do payload e forçar o ID do token.

#### Task 2.2: Emissão do Token JWT e Atualização do Middleware (Backend)
**Descrição:** Substituir o extrator temporário da US01 por um validador real de JWT, devolvendo o token assinado no login.
**Critérios de Aceite (DoD):**
*   [ ] A rota de login deve validar o hash contra o banco de dados. Se correto, emitir JWT assinado (HMAC-SHA256 ou EdDSA).
*   [ ] O payload (Claims) do JWT deve conter obrigatoriamente: `sub` (user_id), `store_id`, `role` e `exp` (expiração curta, máx 12 horas).
*   [ ] O Middleware (US01) deve ser reescrito para capturar o header `Authorization: Bearer <token>`, decodificar, validar assinatura e expiração.
**Cenários de Teste (QA & Segurança):**
*   **Edge Case (Expiração):** Autenticar, esperar expiração (ou alterar a hora local), requisições seguintes devem ser barradas com HTTP 401 Unauthorized.
*   **Security Case (Tampering):** Capturar JWT de `CASHIER`, alterar em texto claro para `MANAGER` e enviar. A API deve rejeitar por quebra de assinatura.

#### Task 2.3: Controle de Acesso Baseado em Papéis (RBAC - Fullstack)
**Descrição:** Garantir que um usuário logado não consiga realizar ações que não pertencem ao seu cargo.
**Critérios de Aceite (DoD):**
*   [ ] Backend: Rotas críticas (`POST /products`, `GET /reports`) exigem `role` extraído do JWT como `MANAGER` ou superior, retornando HTTP 403 caso contrário.
*   [ ] Frontend (React): Botões/menus de relatórios e configurações não devem ser renderizados se o `role` armazenado no Zustand for `CASHIER`.
**Cenários de Teste (QA & Segurança):**
*   **Security Case:** Logar como Caixa. A tela de relatórios não aparece visualmente. Chamar a rota `GET /reports` via Postman usando o token do Caixa. A API deve bloquear com 403.

#### Task 2.4: Armazenamento Seguro de Credenciais no Desktop (Tauri)
**Descrição:** Proteger o token JWT recebido para que não seja extraído da máquina do lojista facilmente.
**Critérios de Aceite (DoD):**
*   [ ] Proibido salvar o JWT no `localStorage` ou `sessionStorage` do navegador embutido do Tauri.
*   [ ] React envia o token ao motor Rust (via IPC) imediatamente após login.
*   [ ] Rust usa biblioteca de armazenamento seguro nativo (Windows Credential Manager / Secret Service) para guardar o cofre criptografado.
*   [ ] Na abertura do app, Rust recupera o token e o disponibiliza para o React montar o cliente HTTP.
**Cenários de Teste (QA & Segurança):**
*   **Isolamento de Memória:** Inspecionar o Application tab do Developer Tools do WebView2. Não deve haver traços do token em texto plano.

### US03: Catálogo de Produtos e Parâmetros Fiscais
**Épico:** Fundação SaaS | **Componente:** Fullstack (Rust/React) | **Risco:** Médio (Integridade Fiscal e Contábil)

**Contexto de Negócio:**
O cadastro de produtos é o coração da operação. A SEFAZ exige rigor absoluto nos parâmetros fiscais (NCM, CFOP, CEST) para autorizar a nota. Além disso, precisamos proteger o lojista contra fraudes de alteração de preço, registrando cada mudança. Como definimos a exclusão lógica, deletar um produto não pode quebrar o banco de dados.

#### Task 3.1: CRUD de Produtos e Validação Fiscal (Backend Axum)
**Descrição:** Criar os endpoints REST (`POST`, `GET`, `PUT`, `DELETE`) para a entidade `PRODUCT`, garantindo a validação de regras fiscais e a injeção do `store_id`.
**Critérios de Aceite (DoD):**
*   [ ] O endpoint `POST /products` deve validar se o `ncm_code` possui exatamente 8 dígitos numéricos.
*   [ ] O endpoint `DELETE /products/:id` não deve executar exclusão física. Deve atualizar a coluna `is_active = false` (Soft Delete).
*   [ ] A listagem `GET /products` deve retornar apenas produtos onde `is_active = true` e `store_id` corresponda ao tenant do usuário autenticado.
**Cenários de Teste (QA & Segurança):**
*   **Edge Case:** Tentar enviar um NCM com letras ou tamanho incorreto. A API deve retornar HTTP 422 Unprocessable Entity.
*   **Security Case:** Acessar a rota `DELETE` com papel de `CASHIER`. O middleware deve bloquear (HTTP 403), permitindo apenas `MANAGER` ou `ADMIN`.

#### Task 3.2: Gatilho de Auditoria de Preços via Transação (Backend SQLx)
**Descrição:** Implementar a lógica que monitora alterações na coluna `selling_price` e grava o evento no `AUDIT_LOG` na mesma transação atômica.
**Critérios de Aceite (DoD):**
*   [ ] No endpoint `PUT /products/:id`, comparar o `selling_price` do payload com o valor atual no banco.
*   [ ] Se houver diferença, iniciar uma `sqlx::Transaction`. Atualizar o produto e inserir um registro na `AUDIT_LOG` com `action_type = "PRICE_CHANGE"`, guardando o valor antigo e novo no campo `JSONB`.
*   [ ] Se a inserção no log falhar, toda a transação deve sofrer *rollback*.
**Cenários de Teste (QA):**
*   **Happy Path:** Alterar o preço de R$ 10 para R$ 15. Verificar no banco se a tabela `AUDIT_LOG` contém o `user_id` de quem alterou e a divergência de valores.

#### Task 3.3: Interface de Cadastro de Produto (Frontend React)
**Descrição:** Criar a tela do catálogo com formulários tipados para garantir que o usuário preencha as exigências fiscais.
**Critérios de Aceite (DoD):**
*   [ ] Formulário dividido em abas ou seções: "Dados Gerais" (Nome, EAN, Preços) e "Fiscal" (NCM, CEST, CFOP Padrão, Origem).
*   [ ] Máscaras de input aplicadas para código de barras (GTIN/EAN) e formatação monetária padrão BRL.
*   [ ] Integração com a Cloud API enviando o token JWT no cabeçalho.

---

### US04: Motor de Estoque (Ledger Imutável)
**Épico:** Fundação SaaS | **Componente:** Cloud API (Rust) | **Risco:** Alto (Consistência de Dados)

**Contexto de Negócio:**
O estoque não é apenas um número estático; é uma conta corrente. Não podemos simplesmente fazer `UPDATE STOCK SET quantity = 10`. Precisamos gravar o *movimento* (entrada, saída, perda) para compor o saldo final, formando uma trilha de auditoria infalível (Ledger).

#### Task 4.1: Repositório de Movimentação (STOCK_MOVEMENT)
**Descrição:** Criar os métodos no Rust para registrar entradas e saídas de estoque sempre anexadas a uma justificativa (`movement_type`).
**Critérios de Aceite (DoD):**
*   [ ] Criar função `insert_movement` que exija: `product_id`, `store_id`, `change_amount` (positivo ou negativo) e `movement_type` (ex: `SALE`, `MANUAL_ADD`, `RETURN`).
*   [ ] O saldo final na tabela `STOCK` deve ser atualizado via *Trigger* no PostgreSQL ou via Transação explícita no SQLx sempre que um movimento for inserido.
**Cenários de Teste (QA):**
*   **Happy Path:** Inserir um movimento de `+50` (MANUAL_ADD). O saldo em `STOCK` passa a ser 50. Inserir movimento de `-2` (SALE). O saldo vai para 48.

#### Task 4.2: API de Ajuste Manual e Inventário
**Descrição:** Rota específica para o gerente dar entrada em notas de fornecedores ou relatar perdas (quebra/validade).
**Critérios de Aceite (DoD):**
*   [ ] Endpoint `POST /stock/adjust` que recebe um array de itens, quantidades e o motivo do ajuste.
*   [ ] Permissão estrita para perfis gerenciais.
**Cenários de Teste (QA):**
*   **Edge Case:** Tentar inserir um ajuste que deixe o saldo negativo em produtos não configurados para permitir saldo negativo. A transação deve falhar e alertar o usuário.

---

### US05: Setup do Container Desktop (Tauri + React)
**Épico:** Frente de Caixa | **Componente:** Ambiente Multiplataforma | **Risco:** Baixo (Configuração)

**Contexto de Negócio:**
Esta é a base do cliente desktop. A aplicação precisa rodar como um processo nativo do sistema operacional (usando o WebView2 no Windows), minimizando o consumo de memória, garantindo agilidade no caixa e preparando o terreno para comunicação com hardware (impressoras).

#### Task 5.1: Inicialização do Workspace Tauri e Vite
**Descrição:** Criar a estrutura do `desktop-client` utilizando Vite (React + TypeScript) e configurar o invólucro do Tauri.
**Critérios de Aceite (DoD):**
*   [ ] Executar o scaffold do Tauri limitando as permissões de IPC apenas aos comandos necessários no `tauri.conf.json`.
*   [ ] Configurar o Tailwind CSS para estilização e o Zustand para o estado global do frontend.
*   [ ] O build do aplicativo (modo dev) deve inicializar a janela nativa sem depender de browsers externos.
**Cenários de Teste (QA):**
*   **Happy Path:** Executar `cargo tauri dev`. A janela do aplicativo abre renderizando o "Hello World" do React, consumindo menos de 80MB de RAM no gerenciador de tarefas.

#### Task 5.2: Configuração de Segurança IPC e Variáveis de Ambiente
**Descrição:** Proteger a comunicação entre o React e o motor local em Rust, preparando o roteamento para a API na nuvem (ex: Render/AWS).
**Critérios de Aceite (DoD):**
*   [ ] Isolar URLs da API de produção e desenvolvimento utilizando `.env`.
*   [ ] Desabilitar o acesso ao `window.__TAURI__` no console do desenvolvedor em builds de produção para evitar execução arbitrária de comandos locais.

---

### US06: Carrinho e Leitura de Código de Barras
**Épico:** Frente de Caixa | **Componente:** Frontend (Zustand/React) | **Risco:** Médio (UX e Performance)

**Contexto de Negócio:**
A tela do caixa precisa ser ultrarrápida. Cada leitura de código de barras deve adicionar o produto ao carrinho instantaneamente. Não podemos fazer um request HTTP para a nuvem a cada "bip", portanto, o catálogo deve estar em cache ou ser consultado com latência mínima, e o estado mantido localmente.

#### Task 6.1: Gerenciamento de Estado do Carrinho (Zustand)
**Descrição:** Criar a store local para gerenciar os itens bipados antes de consolidar a venda.
**Critérios de Aceite (DoD):**
*   [ ] Criar `useCartStore` contendo array de `SaleItem` (com `unit_price`, `quantity`, `discount_amount`, e os snapshots fiscais).
*   [ ] Implementar ações: `addItem`, `removeItem`, `applyDiscount`, `clearCart`.
*   [ ] O cálculo do `total_items_amount` e `total_amount` deve ser derivado (computed) automaticamente com base no array de itens, evitando inconsistências matemáticas.
**Cenários de Teste (QA):**
*   **Happy Path:** Adicionar 2 unidades do Produto A (R$ 10). O total deve refletir R$ 20 instantaneamente. Aplicar desconto de R$ 5 no item. Total muda para R$ 15.

#### Task 6.2: Hook de Captura de Código de Barras (Hardware Dummy)
**Descrição:** Leitores USB funcionam simulando um teclado que digita rapidamente e aperta `Enter`. O sistema precisa escutar esse padrão globalmente na tela de vendas.
**Critérios de Aceite (DoD):**
*   [ ] Criar um hook customizado `useBarcodeScanner` que escuta eventos de `keydown`.
*   [ ] Identificar uma sequência rápida de números finalizada pela tecla `Enter` para distinguir de uma digitação humana comum.
*   [ ] Ao capturar o código, buscar o produto no cache/API e disparar o `addItem` da Task 6.1 automaticamente.
**Cenários de Teste (QA):**
*   **Simulação de Hardware:** Focar na tela do PDV e digitar rapidamente "7891020304050" + `Enter`. O produto correspondente deve saltar para o grid do carrinho sem que o usuário precise clicar em nenhum input de texto.

### US07: Pagamentos Fracionados (Split Payment)
**Épico:** Frente de Caixa | **Componente:** Fullstack (Tauri/React + Rust) | **Risco:** Alto (Consistência Financeira)

**Contexto de Negócio:**
Clientes frequentemente dividem o pagamento (ex: metade no cartão, metade em dinheiro). O sistema deve ser capaz de registrar múltiplos métodos para uma única venda, garantindo que o valor total pago corresponda ao valor cobrado antes de liberar a impressão do comprovante e a baixa no estoque.

#### Task 7.1: Motor de Pagamento Fracionado (Backend Rust)
**Descrição:** Atualizar o endpoint de fechamento de venda (`POST /sales`) para aceitar um array de pagamentos e validar a integridade matemática da transação.
**Critérios de Aceite (DoD):**
*   [ ] O payload deve receber um array de objetos `payments` contendo `method`, `tendered_amount` (valor entregue) e `amount` (valor consumido).
*   [ ] O backend deve iterar sobre o array e somar o `amount` de cada método. A venda só pode ser registrada se a soma exata dos `amount` for igual ao `total_amount` (líquido) da venda.
*   [ ] Se o método for diferente de `CASH` (Dinheiro), o `tendered_amount` não pode ser maior que o `amount` (não há troco em PIX/Cartão).
**Cenários de Teste (QA & Segurança):**
*   **Edge Case (Pagamento Menor):** Enviar array totalizando R$ 40 para uma venda de R$ 50. A API deve barrar com HTTP 422.
*   **Security Case (Fraude de Troco):** Tentar enviar `tendered_amount = 100` e `amount = 50` em um pagamento via `PIX`. A transação deve ser bloqueada.

#### Task 7.2: Interface de Múltiplos Pagamentos (Frontend React)
**Descrição:** Criar o modal de *checkout* que permite ao operador ir adicionando pagamentos até que o saldo devedor zere.
**Critérios de Aceite (DoD):**
*   [ ] O modal deve exibir o "Valor Total", "Valor Pago até agora" e "Falta Pagar".
*   [ ] Botões rápidos para métodos de pagamento (Dinheiro, PIX, Crédito, Débito).
*   [ ] O botão "Finalizar Venda" deve permanecer bloqueado (disabled) até que o "Falta Pagar" seja menor ou igual a zero.

---

### US08: Cálculos de Caixa e Troco
**Épico:** Frente de Caixa | **Componente:** Frontend (React) | **Risco:** Médio (UX e Matemática)

**Contexto de Negócio:**
O operador precisa de feedback visual imediato para dar o troco corretamente. Erros de troco sangram o caixa da loja. O cálculo deve ser automático e à prova de falhas operacionais.

#### Task 8.1: Lógica de Troco Automático
**Descrição:** Calcular o troco em tempo real no Zustand store assim que o operador digitar o valor entregue em dinheiro.
**Critérios de Aceite (DoD):**
*   [ ] Se o "Falta Pagar" for R$ 20 e o operador selecionar "Dinheiro" e digitar "R$ 50", a tela deve imediatamente exibir "Troco: R$ 30,00" em destaque.
*   [ ] No payload a ser enviado para a API, este pagamento deve ir como `tendered_amount: 50`, `amount: 20`, para que o backend grave a diferença na tabela `SALE` (`change_amount: 30`).
**Cenários de Teste (QA):**
*   **Happy Path:** Operador digita valor superior ao devido. Troco é calculado e venda é liberada.
*   **Edge Case:** Operador digita exatamente o valor devido. Troco deve mostrar R$ 0,00 e liberar a venda.

---

### US09: Perfil da Loja e Certificado Digital (SEFAZ)
**Épico:** Módulo Fiscal (O Chefe Final) | **Componente:** Cloud API (Rust) | **Risco:** Alto (Vazamento de Chave Criptográfica)

**Contexto de Negócio:**
Para assinar XMLs com valor legal, a API precisa do certificado digital e-CNPJ (A1) da loja. Esse arquivo `.pfx` e sua senha são altamente sensíveis e dão plenos poderes sobre o CNPJ da empresa. O armazenamento deve ser blindado.

#### Task 9.1: Upload e Armazenamento do Certificado
**Descrição:** Criar rota e mecanismo seguro para upload do `.pfx` e armazenamento de sua senha.
**Critérios de Aceite (DoD):**
*   [ ] O arquivo `.pfx` deve ser validado via parse antes de ser salvo (para garantir que não é um malware renomeado).
*   [ ] A senha do certificado (`certificate_password`) deve ser criptografada em repouso no banco de dados usando uma chave mestra (AES-256) armazenada nas variáveis de ambiente (`.env`) do servidor. Não deve ficar em texto plano.
*   [ ] Acesso a esta tela e rota é exclusivo para o papel `ADMIN/DONO`.
**Cenários de Teste (Security):**
*   **Security Case:** Tentar baixar ou visualizar a senha do certificado através de uma rota `GET`. A API nunca deve expor a senha de volta ao frontend; ela é usada apenas internamente pelo motor fiscal.

---

### US10: Geração e Assinatura do XML (NFC-e)
**Épico:** Módulo Fiscal | **Componente:** Cloud API (Rust) | **Risco:** Alto (Rejeição SEFAZ)

**Contexto de Negócio:**
A venda física virou um dado no banco. Agora ela precisa virar um documento fiscal no padrão brasileiro, ser assinado digitalmente e transmitido em tempo real aos servidores do governo.

#### Task 10.1: Montagem do XML (Motor Fiscal)
**Descrição:** Mapear a venda finalizada (`SALE` e `SALE_ITEM`) para as tags obrigatórias do manual do contribuinte (NF-e/NFC-e).
**Critérios de Aceite (DoD):**
*   [ ] Gerar os nós `<ide>`, `<emit>`, `<dest>`, `<det>` e `<total>` de acordo com o Tax Snapshot dos itens.
*   [ ] Em caso de CPF na Nota para consumidor não cadastrado, utilizar a coluna `anonymous_cpf_cnpj`.
*   [ ] Se os impostos estiverem zerados ou malformados, o processo deve abortar *antes* de enviar à SEFAZ, devolvendo um erro amigável ao caixa ("Produto X sem NCM").

#### Task 10.2: Assinatura RSA e Envio Síncrono (SEFAZ)
**Descrição:** Assinar o XML com o certificado A1 e realizar o post no web service do governo estadual correspondente à loja.
**Critérios de Aceite (DoD):**
*   [ ] O nó `<Signature>` deve ser gerado utilizando o certificado descriptografado em memória.
*   [ ] Implementar rotina de *retry* exponencial (máximo de 3 tentativas) para instabilidades de rede.
*   [ ] O retorno de autorização (Protocolo e Chave de 44 dígitos) deve ser salvo na tabela `SALE` e liberado para impressão.
**Cenários de Teste (QA):**
*   **Happy Path (Homologação):** Enviar venda válida, receber cStat 100 (Autorizado o uso) e gravar chave.
*   **Edge Case (Rejeição):** Simular rejeição por erro de NCM (ex: cStat 778). O sistema deve marcar a nota como rejeitada e não imprimir o cupom final.

---

### US11: Exportação de XML para o Contador
**Épico:** Módulo Fiscal | **Componente:** Fullstack (Rust/React) | **Risco:** Baixo

**Contexto de Negócio:**
No 1º dia útil de cada mês, o dono da loja precisa enviar todas as notas emitidas e canceladas para o seu contador calcular os impostos da empresa.

#### Task 11.1: Agrupador Mensal (Worker/Endpoint)
**Descrição:** Rota para zipar todos os arquivos físicos e gerar download.
**Critérios de Aceite (DoD):**
*   [ ] Rota `GET /fiscal/export?month=10&year=2026`.
*   [ ] O backend deve buscar todas as vendas do `store_id` daquele período que possuem `fiscal_xml_url` e agrupar em um único `.zip`.
*   [ ] O arquivo deve ser montado na memória (Stream) e servido como download, sem onerar o disco do servidor na nuvem.

---

### US12: Impressora Térmica Direta (Hardware via Tauri)
**Épico:** Frente de Caixa | **Componente:** Tauri IPC (Rust Local) | **Risco:** Alto (Conectividade Física)

**Contexto de Negócio:**
O lojista quer apertar "Vender" e ouvir a impressora imprimir na hora. Aplicativos web normais abrem a janela de impressão do Windows (o que atrasa a fila). O Tauri deve enviar dados direto para a porta USB/Serial da impressora térmica.

#### Task 12.1: Módulo de Comunicação ESC/POS (Rust Local)
**Descrição:** Criar um comando Tauri que recebe os dados do DANFE (Documento Auxiliar) e converte em bytes nativos de impressão térmica.
**Critérios de Aceite (DoD):**
*   [ ] Implementar função Rust no `src-tauri` para buscar portas USB ativas ou utilizar driver Spooler em modo RAW.
*   [ ] Formatar texto, negrito, alinhamento e o QR Code (obrigatório na NFC-e) usando protocolo ESC/POS.
*   [ ] Criar função `invoke('print_receipt', { data })` para o React chamar.
**Cenários de Teste (QA):**
*   **Teste de Integração:** Interceptar a impressão e enviar para uma impressora térmica genérica (Daruma, Bematech ou emulador). O papel deve sair sem falhas de acentuação e o QR Code deve ser legível por smartphone.

---

### US13: Fila Assíncrona e Contingência Offline
**Épico:** Frente de Caixa | **Componente:** Tauri (SQLite/Rust Local) | **Risco:** Crítico (Arquitetura Distribuída)

**Contexto de Negócio:**
Se o servidor da SEFAZ cair (ou a internet da loja parar), o supermercado não pode fechar. A lei permite a emissão "Offline" (Contingência), onde o sistema gera e imprime a nota na hora e avisa o governo horas depois.

#### Task 13.1: Banco de Dados Local para Fila (SQLite)
**Descrição:** Configurar um banco embutido no cliente desktop para segurar vendas que não conseguiram chegar na nuvem.
**Critérios de Aceite (DoD):**
*   [ ] O motor Tauri deve instanciar um arquivo `offline_queue.db` via SQLite.
*   [ ] Se a chamada de venda à Cloud API dar Timeout ou retornar 503/500, o frontend deve delegar o payload inteiro da venda para o Tauri através do comando `invoke('save_offline_sale')`.
*   [ ] A impressora (US12) deve emitir o cupom com a frase "EMITIDA EM CONTINGÊNCIA - PENDENTE DE AUTORIZAÇÃO" (Exigência legal).

#### Task 13.2: Worker de Sincronização
**Descrição:** Um processo em background que tenta esvaziar a fila assim que a internet volta.
**Critérios de Aceite (DoD):**
*   [ ] No `src-tauri`, iniciar um worker em thread separada que pinga a Cloud API a cada 3 minutos.
*   [ ] Se houver conexão, pegar os registros do SQLite e enviar para a nuvem. Em caso de sucesso (HTTP 201), apagar do SQLite local.
*   [ ] A loja deve possuir um ícone no header do React mostrando "Modo Online (Verde)" ou "Modo Contingência (Laranja - X pendentes)".

### US14: Turnos do Caixa (Abertura e Fechamento de Gaveta)
**Épico:** Controle de Caixa e Tesouraria | **Componente:** Fullstack (Rust/React) | **Risco:** Alto (Fraude e Desvio Financeiro)

**Contexto de Negócio:**
O dinheiro na gaveta precisa bater centavo por centavo com o que o sistema registrou. Para isso, o caixa não pode operar de forma contínua 24/7. Ele deve iniciar um "Turno" (Shift) informando com quanto dinheiro começou (Fundo de Troco) e, no final do dia, deve contar as notas e declarar o valor cego (Blind Close), para que o gerente valide se houve quebra (sobra ou falta).

#### Task 14.1: Gerenciamento de Sessão de Caixa (Backend)
**Descrição:** Implementar a lógica da tabela `CASH_SHIFT` para garantir que um usuário não possa ter dois turnos abertos simultaneamente e que as vendas sejam vinculadas ao turno ativo.
**Critérios de Aceite (DoD):**
*   [ ] Rota `POST /shifts/open` deve receber o `opening_balance`. O sistema deve validar se o `cashier_id` já possui um turno com `closed_at = null`. Se sim, rejeitar com HTTP 409 Conflict.
*   [ ] O endpoint de registro de Venda (`POST /sales`) agora deve verificar se o caixa autenticado possui um turno aberto. Se não possuir, a venda deve ser bloqueada.
*   [ ] Rota `POST /shifts/close` deve receber o `actual_closed_balance` (declarado pelo operador). O backend calcula a diferença entre o esperado e o declarado e encerra o turno carimbando a data final.
**Cenários de Teste (QA):**
*   **Edge Case:** Tentar registrar uma venda no turno do dia anterior que o operador esqueceu de fechar, mas após 24h. O sistema deve alertar o gerente.
*   **Security Case:** Tentar forçar o fechamento de um turno pertencente a outro operador através de manipulação de payload. O backend deve barrar comparando o token JWT.

#### Task 14.2: Tela de Abertura e Fechamento Cego (Frontend React)
**Descrição:** Criar a interface de bloqueio do PDV que impede a operação antes da abertura da gaveta.
**Critérios de Aceite (DoD):**
*   [ ] Se o estado global (`Zustand`) não identificar um `shift_id` ativo, a tela de bipar produtos deve ser bloqueada por um Modal de Abertura.
*   [ ] No fechamento, a tela não deve mostrar o "Valor Esperado". O operador precisa contar o dinheiro e digitar quanto achou. O sistema mostrará a quebra (falta/sobra) apenas no relatório do gerente.

---

### US15: Movimentações Avulsas (Sangria e Suprimento)
**Épico:** Controle de Caixa e Tesouraria | **Componente:** Fullstack (Rust/React) | **Risco:** Médio (Auditoria)

**Contexto de Negócio:**
Durante o expediente, pode faltar troco (Suprimento: o gerente coloca mais R$ 100 na gaveta) ou acumular muito dinheiro (Sangria: o gerente retira R$ 1.000 da gaveta e guarda no cofre por segurança). Essas ações afetam o cálculo do fechamento do caixa (US14).

#### Task 15.1: Registro de Movimentos na Gaveta (CASH_MOVEMENT)
**Descrição:** Endpoints e lógica para registrar entradas e saídas físicas que não provêm de vendas.
**Critérios de Aceite (DoD):**
*   [ ] Rota `POST /shifts/movement` exigindo `movement_type` (SANGRIA, SUPRIMENTO) e um motivo obrigatório (`description`).
*   [ ] Deve vincular obrigatoriamente a um `shift_id` em aberto.
*   [ ] Inserir uma trava: O valor de uma Sangria nunca pode ser superior ao valor atual em dinheiro físico (`CASH`) calculado pelo sistema naquele momento do turno.
**Cenários de Teste (QA):**
*   **Edge Case:** Tentar sangrar R$ 500 quando a gaveta tem apenas R$ 300 registrados (Fundo Inicial + Vendas em Dinheiro). A API deve impedir com HTTP 422.

---

### US16: Contas a Pagar e Fornecedores
**Épico:** Retaguarda Financeira | **Componente:** Cloud API (Rust) | **Risco:** Baixo (CRUD Padrão)

**Contexto de Negócio:**
O varejista precisa saber se a loja está dando lucro. Para isso, não basta registrar as vendas; é preciso registrar o pagamento de fornecedores (cerveja, embalagens) e contas de consumo (água, luz). 

#### Task 16.1: Módulo de Despesas (EXPENSE e SUPPLIER)
**Descrição:** Implementar os endpoints de cadastro de fornecedores (Pessoa Física e Jurídica) e lançamentos de contas a pagar.
**Critérios de Aceite (DoD):**
*   [ ] Rota `POST /suppliers` validando `cnpj_cpf` do fornecedor ou produtor rural.
*   [ ] Rota `POST /expenses` para registrar o boleto, contendo `due_date`, `amount` e status (`PENDING` ou `PAID`).
*   [ ] Se uma despesa for paga utilizando dinheiro da gaveta do caixa, o endpoint de pagamento deve, na mesma transação SQLx, inserir um `CASH_MOVEMENT` (tipo DESPESA) no turno ativo correspondente.
**Cenários de Teste (QA):**
*   **Integração Contábil:** Pagar uma despesa de R$ 50 com dinheiro do caixa. Verificar se, no fechamento do turno, o valor esperado em dinheiro caiu em exatos R$ 50, não causando "falta" para o operador.

---

### US17: Dashboard Financeiro Consolidado
**Épico:** Retaguarda Financeira | **Componente:** Fullstack (Rust/React) | **Risco:** Médio (Performance de Banco)

**Contexto de Negócio:**
O dono da loja quer acessar a retaguarda web de casa e ver um gráfico mostrando o faturamento líquido, descontando os custos (produtos e despesas), sem precisar exportar para o Excel.

#### Task 17.1: Agregadores de Fluxo de Caixa (Consultas SQLx)
**Descrição:** Escrever consultas SQL complexas utilizando `GROUP BY` e agregadores (`SUM`) para retornar dados consolidados por período, cuidando da performance.
**Critérios de Aceite (DoD):**
*   [ ] Criar endpoint `GET /reports/cashflow?start_date=X&end_date=Y`.
*   [ ] A consulta SQL deve cruzar a tabela `SALE` (status = COMPLETED) somando os recebimentos, subtraindo o custo da mercadoria (`cost_price` histórico na tabela de produtos, se rastreado) e subtraindo a tabela `EXPENSE` (status = PAID).
*   [ ] Utilizar índices adequados no PostgreSQL (ex: criar um índice em `created_at` na tabela `SALE`) para que a consulta de um ano inteiro não dê *timeout*.
**Cenários de Teste (Performance):**
*   **Stress Test:** Popular o banco de dados com 100.000 vendas fictícias utilizando um script. Chamar a rota do relatório consolidado e garantir que ela responda em menos de 800ms.

#### Task 17.2: Gráficos de Faturamento (Frontend)
**Descrição:** Utilizar uma biblioteca de visualização de dados (ex: Recharts, Chart.js) para criar painéis interativos.
**Critérios de Aceite (DoD):**
*   [ ] Exibir *Cards* de consolidação (Faturamento Bruto, Lucro Bruto, Ticket Médio).
*   [ ] Exibir um gráfico de barras cruzando "Entradas vs Saídas" diárias.
*   [ ] Criar um filtro de datas que evite recarregamento da página toda vez que o usuário alterar o período, fazendo uso de hooks para buscar dados dinamicamente (`SWR` ou `React Query`).

### US18: Gestão de Fiado (Conta Corrente / Caderneta)
**Épico:** Retenção e Relacionamento | **Componente:** Fullstack (Rust/React) | **Risco:** Alto (Inadimplência e Bloqueio)

**Contexto de Negócio:**
O cliente da casa leva a mercadoria hoje e paga no fim do mês. Como a mercadoria saiu fisicamente da loja, o estoque deve ser baixado. No entanto, a NFC-e (imposto) só será gerada quando ele vier pagar a conta. Se a compra ultrapassar o limite de crédito do cliente, o sistema trava e chama o gerente.

#### Task 18.1: Registro de Venda Pendente (Backend)
**Descrição:** Adaptar o fluxo de registro de vendas para aceitar transações "Fiado".
**Critérios de Aceite (DoD):**
*   [ ] Ao receber um payload com `transaction_type = 'SALE'` e status `PENDING`, a API deve pular a fila de geração do XML Fiscal (US10).
*   [ ] O sistema deve inserir a venda, os itens, não registrar pagamento (ou registrar como "A PRAZO") e **deve** gerar a saída no `STOCK_MOVEMENT`, pois a peça saiu da loja.
*   [ ] O payload deve conter obrigatoriamente um `customer_id` válido. Venda pendente não pode ser anônima.

#### Task 18.2: Validação e Trava de Limite de Crédito (Backend)
**Descrição:** Impedir que o saldo devedor de um cliente estoure o limite acordado sem autorização.
**Critérios de Aceite (DoD):**
*   [ ] Antes de gravar a venda `PENDING`, o backend deve somar todas as vendas com status `PENDING` daquele cliente e adicionar o valor da compra atual.
*   [ ] Se a soma for maior que o `credit_limit` da tabela `CUSTOMER`, o backend deve rejeitar a transação com HTTP 403 (ou código específico), exigindo o `override_pin` gerencial para prosseguir (integração com US21).

---

### US19: Quitação de Débitos e Emissão Posterior
**Épico:** Retenção e Relacionamento | **Componente:** Fullstack (Rust/React) | **Risco:** Médio (Fiscal)

**Contexto de Negócio:**
Chegou o final do mês. O "Seu João" vem à loja pagar a conta que acumulou R$ 350 em 4 compras diferentes. O lojista recebe o dinheiro, zera a dívida e agora sim o sistema emite a NFC-e para a SEFAZ.

#### Task 19.1: Extrato do Cliente (React)
**Descrição:** Tela no PDV para consultar clientes devedores e selecionar as compras para pagamento.
**Critérios de Aceite (DoD):**
*   [ ] Uma interface de busca por Nome/CPF que traga o saldo devedor total.
*   [ ] Exibir um grid com todas as vendas em status `PENDING`.
*   [ ] O operador pode selecionar uma, várias ou todas as vendas pendentes para quitar de uma só vez.

#### Task 19.2: Consolidação e Gatilho Fiscal (Backend)
**Descrição:** Endpoint para processar o pagamento e engatilhar a nota.
**Critérios de Aceite (DoD):**
*   [ ] Rota `POST /customers/:id/pay` que recebe o array de `sale_id` e os dados do `PAYMENT` (Dinheiro, PIX, etc.).
*   [ ] O backend altera o status dessas vendas de `PENDING` para `COMPLETED`.
*   [ ] O backend injeta as vendas pagas na fila de geração de XML (US10) de forma assíncrona.
*   [ ] Nenhuma alteração no `STOCK` deve ocorrer aqui, pois já foi reduzido na US18.

---

### US20: Emissão de Orçamentos (Informacional)
**Épico:** Retenção e Exceções | **Componente:** Fullstack (Rust/React) | **Risco:** Baixo

**Contexto de Negócio:**
O cliente quer apenas saber quanto custaria um conjunto de peças (ex: material de construção, tintas). O orçamento é um documento impresso temporário, não reserva mercadoria no estoque e os preços nele não são garantidos eternamente.

#### Task 20.1: Gravação e Impressão de Orçamento
**Descrição:** Fluxo separado para gerar um documento sem impacto operacional.
**Critérios de Aceite (DoD):**
*   [ ] O endpoint de registro deve aceitar `transaction_type = 'BUDGET'`.
*   [ ] Vendas com tipo `BUDGET` **não** geram `STOCK_MOVEMENT`, **não** geram XML e **não** afetam o turno do caixa (`CASH_SHIFT`).
*   [ ] O comando Tauri de impressão (US12) deve ser adaptado. Se o tipo for `BUDGET`, imprimir no cabeçalho "ORÇAMENTO - NÃO É DOCUMENTO FISCAL" e no rodapé "Preços sujeitos a alteração. Válido por 7 dias."

---

### US21: Override Gerencial (Autorização por PIN)
**Épico:** Retenção e Exceções | **Componente:** Cloud API / Tauri | **Risco:** Crítico (OWASP A04)

**Contexto de Negócio:**
Toda operação que gera risco financeiro para a loja (desconto exagerado, estourar limite do fiado, cancelar um item já bipado, cancelar uma nota) deve ser travada para o caixa comum e liberada apenas pelo gerente mediante senha local, gerando um registro imutável de auditoria.

#### Task 21.1: Interceptador Front-end (React)
**Descrição:** O "Cadeado Visual" que trava a tela quando uma regra de risco é quebrada.
**Critérios de Aceite (DoD):**
*   [ ] O estado global (Zustand) deve monitorar gatilhos: Excluir item do carrinho, aplicar desconto superior a 10% ou recebimento de erro "Limite Estourado" da US18.
*   [ ] Ao disparar o gatilho, abrir um Modal "Autorização Necessária" com teclado numérico exigindo o PIN do Gerente.

#### Task 21.2: Endpoint de Autorização e Auditoria (Backend)
**Descrição:** Rota sensível que valida a autoridade e carimba o passe livre temporário.
**Critérios de Aceite (DoD):**
*   [ ] Rota `POST /auth/override` recebe o PIN digitado.
*   [ ] A API varre a tabela `USER` buscando usuários ativos com `role = 'MANAGER' ou 'ADMIN'` no mesmo `store_id` e compara os hashes (Argon2).
*   [ ] Em caso de sucesso, insere o registro na tabela `AUDIT_LOG` apontando qual gerente liberou qual ação para qual caixa.
*   [ ] Retorna um token ou assinatura de uso único (One-Time Pass) para que o frontend anexe ao payload da venda/cancelamento, provando para a API que a ação foi autorizada legitimamente (prevenindo bypass via Postman/cURL).