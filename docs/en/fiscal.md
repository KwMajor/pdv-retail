# A1 digital certificate (SEFAZ)

*Versão em português: [../fiscal.md](../fiscal.md).*

The Cloud API stores each store's e-CNPJ A1 certificate to sign fiscal XML
documents (fiscal engine from US10 onward). The `.pfx` and its password grant
full powers over the CNPJ: the API validates the PKCS#12 before accepting it
and stores **both encrypted** with AES-256-GCM.

## Configuration

`CERT_MASTER_KEY` (env): AES-256 master key in base64 (exactly 32 bytes). In
production the boot is refused if it is missing or invalid. Never commit the
real key. Key rotation is not automatic yet: changing the key makes already
stored certificates unreadable (the `.pfx` would need to be uploaded again).

## Routes (exclusive to the `admin`/OWNER role)

### `POST /api/v1/store/certificate`

JSON body (the `.pfx` travels in base64 to keep the JSON/Swagger contract):

```json
{
  "cnpj": "11222333000181",
  "password": "certificate-password",
  "file_name": "certificate.pfx",
  "file_base64": "MIIK..."
}
```

Validations (outside the domain → `422` with a pt-BR message that never echoes
the password):

- file ≤ 5 MB (decoded) and `.pfx`/`.p12` extension;
- valid base64; content must be a legitimate PKCS#12, with the correct
  password (MAC check) and with a certificate + private key pair;
- password from 1 to 128 characters (never logged nor returned);
- CNPJ with exactly 14 digits (no punctuation) and a valid mod-11 DV;
- CNPJ must match the store record (`store_settings.cnpj`, compared by digits
  only) and, when the subject carries a CNPJ (e-CNPJ), the certificate's own.

`200` response (public metadata; never password/PFX):

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

The upload is an **upsert** (one certificate per store): sending again replaces
the previous pair. Each upload writes `audit_log` (`action =
certificate.uploaded`) with metadata only (CNPJ, subject, serial, validity,
actor).

Errors: `401` without JWT, `403` for manager/cashier, `413` body above 8 MiB
(transport limit; plain text), `422` for the validations above.

### `GET /api/v1/store/certificate`

Returns the same public metadata. It never exposes the password or the PFX
bytes; without a certificate → `404`.

## Security guarantees

- Real PKCS#12 validation before persisting (extension alone is not enough).
- PFX and password encrypted at rest with AES-256-GCM, a unique nonce per
  operation and AAD binding the ciphertext to the `store_id` and to the field
  (`pfx`/`password`): copying the row to another store does not decrypt.
- The master key lives only in the process environment — a database dump does
  not expose the secrets.
- Tenant always comes from the JWT; the route is exclusive to `admin` (OWNER).

## UI pending item (Task 9.1 scope)

The Task 9.1 DoD mentions a "screen", but the US09 component is the Cloud API.
This task delivered the route only (used via API/Swagger in development). The
upload screen in the desktop Settings area is deferred to US10 (fiscal engine),
when the full fiscal flow is defined. Recorded here so the requirement is not
lost.
