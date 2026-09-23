# Sistema PDV SaaS (Retail)

## Sobre o Projeto
Este projeto tem como objetivo o desenvolvimento de um aplicativo multiplataforma moderno para operação de frente de caixa (PDV) e retaguarda gerencial. A solução foi desenhada no modelo Multi-Tenant (SaaS), permitindo que múltiplas lojas independentes utilizem a mesma infraestrutura de banco de dados e backend na nuvem com isolamento total de informações. O frontend roda nativamente no sistema operacional via Tauri, permitindo acesso direto ao hardware do lojista com consumo mínimo de recursos.

## Contexto de Aplicação
A solução foi projetada para atender ao pequeno e médio varejo brasileiro (mercados, lojas de conveniência, autopeças, etc). Nesse cenário, o sistema é dimensionado para:

* Trabalhar com operação rápida no balcão, garantindo navegação por teclado e leitor de código de barras.
* Garantir conformidade fiscal absoluta com a SEFAZ para emissão de NFC-e/NF-e.
* Permitir operações offline ou contingenciais para que a loja nunca pare de vender em caso de queda de internet.
* Suportar arquitetura escalável para crescimento do número de lojas atendidas sem impacto na latência do banco de dados (arquitetura isolada por `store_id`).

---

## Stack Tecnológica e Licenciamento
O sistema foi arquitetado visando alta performance, baixo consumo de recursos e segurança jurídica para comercialização (Closed-Source SaaS). Todas as tecnologias e bibliotecas adotadas possuem licenças permissivas open-source (**MIT, Apache 2.0, BSD ou PostgreSQL License**), garantindo que não há exigência de abertura de código ou pagamento de royalties comerciais.

**Backend (API na Nuvem)**
*   **Linguagem:** Rust (MIT / Apache 2.0) — Escolhida pela alta concorrência e uso eficiente de memória.
*   **Framework Web:** Axum (MIT) — Framework assíncrono extremamente rápido para APIs REST.
*   **Acesso a Dados:** SQLx (MIT / Apache 2.0) — Construtor de queries assíncrono e seguro contra SQL Injection, dispensando ORMs pesados.
*   **Gerenciamento de Fila (Mensageria):** Filas baseadas no próprio PostgreSQL ou Valkey (BSD) para comunicação assíncrona com a SEFAZ.

**Frontend & Desktop (O PDV)**
*   **Linguagem:** TypeScript (Apache 2.0).
*   **Container Desktop:** Tauri (MIT / Apache 2.0) — Alternativa mais leve e segura ao Electron, utilizando o renderizador nativo do SO em vez de embutir um navegador inteiro.
*   **Framework UI:** React.js (MIT) — Construção dos componentes visuais e regras de tela.
*   **Estilização:** Tailwind CSS (MIT) e componentes headless como Radix UI (MIT) para construção de interface ágil e acessível.
*   **Gerenciamento de Estado:** Zustand (MIT) — Controle do carrinho de compras e turnos do caixa em memória local.

**Banco de Dados**
*   **SGBD:** PostgreSQL (PostgreSQL License) — Banco relacional robusto com excelente suporte para isolamento de dados via `store_id` (Multi-Tenant) e colunas `JSONB` para logs de auditoria.

---

## Requisitos Funcionais (RF)
* **01 - Multi-Tenancy e Segurança:** Autenticação via JWT, controle de papéis de acesso (Gerente/Caixa) e isolamento rigoroso de dados por lojista.
* **02 - Gestão de Catálogo e Estoque:** Cadastro de produtos com parâmetros fiscais essenciais (NCM, CEST, Origem, CFOP) e razão de movimentação de estoque (Ledger).
* **03 - Frente de Caixa (PDV):** Interface nativa de vendas, suportando múltiplas formas de pagamento em uma mesma compra, orçamentos, descontos e cálculo de troco.
* **04 - Motor Fiscal e SEFAZ:** Integração com certificado digital A1, geração do XML da NFC-e, transmissão via filas assíncronas, impressão térmica ESC/POS e gestão do pacote de arquivos mensais para a contabilidade.
* **05 - Controle de Turnos e Gaveta:** Abertura/fechamento de caixa com conferência cega e registros de sangria/suprimento.
* **06 - Gestão Financeira:** Lançamento de despesas e visualização de saúde financeira através de dashboard de fluxo de caixa.
* **07 - Retenção de Clientes:** Cadastro de clientes para operação de venda a prazo (Fiado) e quitação em lote.
* **08 - Auditoria Operacional:** Registro de trilha de auditoria para ações críticas mediante autorização do gerente via PIN.

## Requisitos Não Funcionais (RNF)
* **Performance do Backend:** A API deve ser desenvolvida em Rust para garantir processamento veloz com baixa latência e concorrência escalável.
* **Baixo Consumo de Memória:** O container desktop deve utilizar o Tauri em conjunto com React para consumir o mínimo possível de RAM na máquina legada do caixa.
* **Comunicação Direta com Hardware:** O aplicativo precisa acessar nativamente a porta USB do SO para acionamento direto de impressoras térmicas locais.
* **Parametrização Fiscal Obrigatória:** A emissão de notas deve bloquear transações que não tenham os códigos fiscais do produto preenchidos e válidos.

---

## Estrutura de Épicos (Backlog no Taiga)

| RF | Rank | Prioridade | User Story | Estimativa | Sprint |
| --- | --- | --- | --- | --- | --- |
| **01** | 1 | Alta | **[US01: Arquitetura Multi-Tenant]** Como desenvolvedor, quero configurar o esquema PostgreSQL exigindo `store_id` nas consultas para isolar os dados das lojas. | 8 | **1** |
| **01** | 2 | Alta | **[US02: Autenticação e Perfis]** Como administrador, quero cadastrar funcionários com papéis (Gerente, Caixa) para restringir o acesso ao sistema via token. | 5 | **1** |
| **02** | 3 | Alta | **[US03: Catálogo e Parâmetros]** Como estoquista, quero cadastrar produtos com dados fiscais. *Critério de Aceite: Qualquer alteração de preço deve gravar um registro na tabela AUDIT_LOG contendo o valor antigo e novo.* | 8 | **1** |
| **02** | 4 | Alta | **[US04: Motor de Estoque]** Como gerente, quero que alterações de quantidade gravem na `STOCK_MOVEMENT` para manter o histórico inalterável do meu estoque. | 8 | **1** |
| **03** | 5 | Alta | **[US05: Container Desktop]** Como caixa, quero abrir o sistema como um .exe leve usando Tauri para garantir fluidez no PC da loja. | 5 | **2** |
| **03** | 6 | Alta | **[US06: Carrinho e Leitor]** Como caixa, quero bipar produtos com o leitor USB para adicioná-los instantaneamente à venda. | 5 | **2** |
| **03** | 7 | Alta | **[US07: Pagamentos Fracionados]** Como cliente, quero dividir o pagamento. *Critério de Aceite (MVP): Pagamentos em cartão serão registrados como Crédito/Débito manualmente, sem comunicação TEF via hardware nesta fase.* | 13 | **2** |
| **03** | 8 | Média | **[US08: Cálculos de Caixa]** Como caixa, quero aplicar descontos no item e visualizar o troco exato na tela para evitar erros. | 5 | **2** |
| **04** | 9 | Alta | **[US09: Perfil da Loja e Certificado]** Como dono, quero subir meu Certificado A1 (.pfx) e configurar meu CNPJ para que a API assine as notas. | 8 | **3** |
| **04** | 10 | Alta | **[US10: Geração do XML]** Como sistema, quero transformar a venda em XML e enviar para a SEFAZ para obter a Chave de Acesso. | 13 | **3** |
| **04** | 11 | Alta | **[US11: Exportação XML Contador]** Como dono da loja, quero selecionar um mês e baixar um arquivo .ZIP com todos os XMLs para enviar diretamente ao meu contador. | 5 | **3** |
| **04** | 12 | Alta | **[US12: Impressora Térmica]** Como caixa, quero que o cupom (DANFE) seja impresso nativamente via comando ESC/POS. | 13 | **3** |
| **04** | 13 | Média | **[US13: Fila Assíncrona e Contingência]** Como gerente, quero emitir notas em contingência offline se a SEFAZ cair para a fila não travar. | 8 | **3** |
| **05** | 14 | Alta | **[US14: Turnos do Caixa]** Como caixa, quero abrir/fechar o turno informando os valores da gaveta para apuração de quebras. | 8 | **4** |
| **05** | 15 | Média | **[US15: Movimentações Avulsas]** Como gerente, quero registrar sangrias ou suprimentos para pagar pequenas despesas na hora. | 3 | **4** |
| **06** | 16 | Média | **[US16: Contas a Pagar]** Como gestor, quero lançar boletos de fornecedores com data de vencimento no sistema. | 5 | **4** |
| **06** | 17 | Média | **[US17: Dashboard Financeiro]** Como dono, quero ver o fluxo de caixa consolidando Receitas vs Despesas em gráficos. | 8 | **4** |
| **07** | 18 | Média | **[US18: Gestão de Fiado]** Como caixa, quero faturar uma venda no crediário (status Pendente) para clientes VIP. | 8 | **5** |
| **07** | 19 | Média | **[US19: Quitação de Débitos]** Como cliente, quero ver meu extrato mensal na loja e quitar as notas pendentes de uma vez. | 5 | **5** |
| **03** | 20 | Baixa | **[US20: Orçamentos]** Como caixa, quero emitir um orçamento impresso sem descontar peças do estoque. | 3 | **5** |
| **08** | 21 | Alta | **[US21: Override Gerente e Devolução]** Como gerente, quero usar meu PIN numérico para autorizar devoluções e descontos elevados com registro total no Log. | 13 | **5** |

---

## Definition of Ready (DoR)
Para que uma User Story seja considerada pronta para entrar em uma Sprint, ela deve obrigatoriamente cumprir os seguintes critérios[cite: 1]:

* [x] **User Story Clara:** A história de usuário descreve o "quem", "o quê" e o "porquê" (valor de negócio)[cite: 1].
* [x] **Regras de Negócio Detalhadas:** As regras associadas à funcionalidade estão descritas[cite: 1].
* [x] **Dados Definidos:** Os dados a armazenar foram definidos, com tipos e validações[cite: 1].
* [x] **Mensagens Definidas:** Mensagens de confirmação, erro e aviso foram especificadas[cite: 1].
* [x] **Esboço de Tela:** Um esboço da(s) tela(s) envolvida(s) foi criado[cite: 1].

## Definition of Done (DoD)
* [x] Código passou por Code Review[cite: 1].
* [x] Pull Request aprovado por outros membros da equipe[cite: 1].
* [x] Testes de regressão executados, sem impacto em funcionalidades existentes[cite: 1].
* [x] Manual do usuário atualizado (quando aplicável)[cite: 1].
* [x] Manual de instalação atualizado (quando aplicável)[cite: 1].

---

## Equipe

|    Função     | Nome                                  |                                                                                                                                                      LinkedIn & GitHub                                                                                                                                                      |
| :-----------: | :------------------------------------ | :-------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------: |
|  Tech Lead / Dev | Matheus Ramos               |   [![Linkedin Badge](https://img.shields.io/badge/Linkedin-blue?style=flat-square&logo=Linkedin&logoColor=white)](https://www.linkedin.com/in/matheusfcrms/) [![GitHub Badge](https://img.shields.io/badge/GitHub-111217?style=flat-square&logo=github&logoColor=white)](https://github.com/KwMajor)   |