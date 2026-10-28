# Certificado digital A1 (SEFAZ)

*English version: [en/fiscal.md](en/fiscal.md).*

A Cloud API guarda o certificado e-CNPJ A1 de cada loja para assinar XMLs
fiscais (motor fiscal a partir da US10). O `.pfx` e a senha dão plenos poderes
sobre o CNPJ: a API valida o PKCS#12 antes de aceitar e grava **os dois
cifrados** com AES-256-GCM.

## Configuração

`CERT_MASTER_KEY` (env): chave mestra AES-256 em base64 (exatamente 32 bytes).
Em produção o boot é recusado se ela faltar ou for inválida. Nunca versione a
chave real. A rotação de chave ainda não é automática: trocar a chave torna os
certificados já gravados ilegíveis (seria necessário reenviar o `.pfx`).

## Rotas (exclusivas do papel `admin`/DONO)

### `POST /api/v1/store/certificate`

Corpo JSON (o `.pfx` vai em base64 para manter o contrato JSON/Swagger):

```json
{
  "cnpj": "11222333000181",
  "password": "senha-do-certificado",
  "file_name": "certificado.pfx",
  "file_base64": "MIIK..."
}
```

Validações (fora do domínio → `422` com mensagem pt-BR, nunca ecoando a senha):

- arquivo ≤ 5 MB (decodificado) e extensão `.pfx`/`.p12`;
- base64 válido; conteúdo precisa ser um PKCS#12 legítimo, com a senha
  correta (confere MAC) e com par certificado + chave privada;
- senha de 1 a 128 caracteres (nunca logada nem devolvida);
- CNPJ com exatamente 14 dígitos (sem pontuação) e DV mod-11 válido;
- CNPJ precisa bater com o cadastro da loja (`store_settings.cnpj`,
  comparado só pelos dígitos) e, quando o subject traz um CNPJ (e-CNPJ), com
  o do certificado.

Resposta `200` (metadados públicos; nunca senha/PFX):

```json
{
  "store_id": "11111111-1111-1111-1111-111111111111",
  "cnpj": "11222333000181",
  "subject_cn": "MINHA LOJA LTDA:11222333000181",
  "serial_hex": "01A2B3C4D5E6F7",
  "not_before": "2026-01-01T00:00:00Z",
  "not_after": "2027-01-01T00:00:00Z",
  "uploaded_at": "2026-10-28T12:00:00Z"
}
```

O upload é **upsert** (1 certificado por loja): reenviar substitui o par
anterior. Cada envio grava `audit_log` (`action = certificate.uploaded`) só
com metadados (CNPJ, subject, serial, validade, autor).

Erros: `401` sem JWT, `403` para manager/caixa, `413` corpo acima de 8 MiB
(limite de transporte; texto puro), `422` validações acima.

### `GET /api/v1/store/certificate`

Devolve os mesmos metadados públicos. Nunca expõe a senha nem os bytes do
PFX; sem certificado → `404`.

## Garantias de segurança

- Validação real do PKCS#12 antes de persistir (extensão sozinha não basta).
- PFX e senha cifrados em repouso com AES-256-GCM, nonce único por operação
  e AAD amarrando o ciphertext ao `store_id` e ao campo (`pfx`/`password`):
  copiar a linha para outra loja não decifra.
- A chave mestra vive só no ambiente do processo — um dump do banco não expõe
  os segredos.
- Tenant vem sempre do JWT; a rota é exclusiva de `admin` (DONO).

## Pendência de interface (escopo da Task 9.1)

O DoD da Task 9.1 cita "tela", mas o componente da US09 é a Cloud API. Nesta
task foi entregue apenas a rota (uso via API/Swagger em desenvolvimento). A
tela de upload na área de Configurações do desktop fica para a US10 (motor
fiscal), quando o fluxo fiscal completo estiver definido. Registrado aqui para
o requisito não se perder.
