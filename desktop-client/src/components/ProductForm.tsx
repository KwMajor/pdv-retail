/**
 * Cadastro de produto em abas (US03 Task 3.3).
 *
 * - "Dados Gerais": nome, SKU, EAN, preço, custo.
 * - "Fiscal": NCM, CEST, CFOP, origem, alíquota.
 * Máscara bloqueante + erro por campo; submit revalida tudo e o backend
 * (autoridade) responde 422 no que passar. Dinheiro sai em centavos→decimal.
 */

import * as Tabs from "@radix-ui/react-tabs";
import { useState } from "react";
import {
  barcodeError,
  centsToDecimal,
  fieldError,
  formatBRLFromCents,
  isValidBarcode,
  isValidNcm,
  ncmError,
  onlyDigits,
} from "../lib/masks";
import { createProduct } from "../lib/products";

type Tab = "geral" | "fiscal";

interface Draft {
  name: string;
  sku: string;
  barcode: string;
  priceCents: string;
  costCents: string;
  ncm: string;
  cest: string;
  cfop: string;
  icmsOrigin: string;
  icmsRate: string;
}

const EMPTY: Draft = {
  name: "",
  sku: "",
  barcode: "",
  priceCents: "",
  costCents: "",
  ncm: "",
  cest: "",
  cfop: "",
  icmsOrigin: "",
  icmsRate: "",
};

function validate(d: Draft): Record<string, string> {
  const errors: Record<string, string> = {};
  const put = (k: string, e: string | null) => {
    if (e) errors[k] = e;
  };
  put("name", fieldError("name", d.name));
  put("sku", fieldError("sku", d.sku));
  put("barcode", barcodeError(onlyDigits(d.barcode)));
  if (d.priceCents === "" || centsToDecimal(d.priceCents) <= 0) {
    errors["price"] = "Preço deve ser maior que zero.";
  }
  put("ncm", ncmError(onlyDigits(d.ncm)));
  put("cest", fieldError("cest", onlyDigits(d.cest)));
  put("cfop", fieldError("cfop", onlyDigits(d.cfop)));
  put("icmsOrigin", fieldError("icms_origin", d.icmsOrigin.trim()));
  if (d.icmsRate.trim() === "" || Number.isNaN(Number(d.icmsRate.replace(",", ".")))) {
    errors["icmsRate"] = "Alíquota deve ser um número.";
  }
  return errors;
}

export function ProductForm({ onCreated }: { onCreated: () => void }) {
  const [tab, setTab] = useState<Tab>("geral");
  const [draft, setDraft] = useState<Draft>(EMPTY);
  const [errors, setErrors] = useState<Record<string, string>>({});
  const [saving, setSaving] = useState(false);
  const [serverError, setServerError] = useState<string | null>(null);

  const set = (k: keyof Draft) => (value: string) => {
    setDraft((d) => ({ ...d, [k]: value }));
  };

  async function submit() {
    const found = validate(draft);
    setErrors(found);
    if (Object.keys(found).length > 0) {
      setTab("geral");
      return;
    }
    setSaving(true);
    setServerError(null);
    try {
      await createProduct({
        name: draft.name.trim(),
        sku: draft.sku.trim(),
        barcode: onlyDigits(draft.barcode) || undefined,
        price: centsToDecimal(draft.priceCents),
        cost: centsToDecimal(draft.costCents),
        ncm: onlyDigits(draft.ncm),
        cest: onlyDigits(draft.cest) || undefined,
        cfop: onlyDigits(draft.cfop) || undefined,
        icms_origin: draft.icmsOrigin.trim() || undefined,
        icms_rate: Number(draft.icmsRate.replace(",", ".")),
      });
      setDraft(EMPTY);
      setTab("geral");
      onCreated();
    } catch (e) {
      setServerError(e instanceof Error ? e.message : "Falha ao salvar.");
    } finally {
      setSaving(false);
    }
  }

  const err = (k: string) =>
    errors[k] ? <span role="alert">{errors[k]}</span> : null;

  return (
    <section aria-label="Cadastro de produto" className="card">
      <Tabs.Root value={tab} onValueChange={(v) => setTab(v as Tab)}>
        <Tabs.List aria-label="Seções do produto">
          <Tabs.Trigger value="geral">Dados Gerais</Tabs.Trigger>
          <Tabs.Trigger value="fiscal">Fiscal</Tabs.Trigger>
        </Tabs.List>

        <Tabs.Content value="geral">
          <div>
          <label>
            Nome*
            <input value={draft.name} onChange={(e) => set("name")(e.target.value)} maxLength={255} />
            {err("name")}
          </label>
          <label>
            SKU*
            <input value={draft.sku} onChange={(e) => set("sku")(e.target.value)} maxLength={64} />
            {err("sku")}
          </label>
          <label>
            EAN/código de barras
            <input
              value={draft.barcode}
              inputMode="numeric"
              onChange={(e) => set("barcode")(onlyDigits(e.target.value).slice(0, 14))}
            />
            {err("barcode")}
          </label>
          <label>
            Preço de venda*
            <input
              value={draft.priceCents === "" ? "" : formatBRLFromCents(draft.priceCents)}
              inputMode="numeric"
              placeholder="R$ 0,00"
              onChange={(e) => set("priceCents")(onlyDigits(e.target.value).slice(0, 12))}
            />
            {err("price")}
          </label>
          <label>
            Custo
            <input
              value={draft.costCents === "" ? "" : formatBRLFromCents(draft.costCents)}
              inputMode="numeric"
              placeholder="R$ 0,00"
              onChange={(e) => set("costCents")(onlyDigits(e.target.value).slice(0, 12))}
            />
          </label>
        </div>
        </Tabs.Content>

        <Tabs.Content value="fiscal">
          <div>
          <label>
            NCM*
            <input
              value={draft.ncm}
              inputMode="numeric"
              maxLength={8}
              onChange={(e) => set("ncm")(onlyDigits(e.target.value).slice(0, 8))}
            />
            {err("ncm")}
          </label>
          <label>
            CEST
            <input
              value={draft.cest}
              inputMode="numeric"
              maxLength={7}
              onChange={(e) => set("cest")(onlyDigits(e.target.value).slice(0, 7))}
            />
            {err("cest")}
          </label>
          <label>
            CFOP
            <input
              value={draft.cfop}
              inputMode="numeric"
              maxLength={4}
              onChange={(e) => set("cfop")(onlyDigits(e.target.value).slice(0, 4))}
            />
            {err("cfop")}
          </label>
          <label>
            Origem ICMS
            <input
              value={draft.icmsOrigin}
              inputMode="numeric"
              maxLength={1}
              onChange={(e) => set("icmsOrigin")(onlyDigits(e.target.value).slice(0, 1))}
            />
            {err("icmsOrigin")}
          </label>
          <label>
            Alíquota ICMS (%)*
            <input
              value={draft.icmsRate}
              inputMode="decimal"
              onChange={(e) => set("icmsRate")(e.target.value.replace(/[^0-9.,]/g, "").slice(0, 6))}
            />
            {err("icmsRate")}
          </label>
        </div>
        </Tabs.Content>
      </Tabs.Root>

      {serverError && <p role="alert">{serverError}</p>}
      <button
        className="primary"
        disabled={
          saving ||
          !isValidNcm(onlyDigits(draft.ncm)) ||
          (draft.barcode !== "" && !isValidBarcode(onlyDigits(draft.barcode)))
        }
        onClick={() => {
          void submit();
        }}
      >
        {saving ? "Salvando…" : "Salvar produto"}
      </button>
    </section>
  );
}
