// ESLint mínimo (bugs, não estilo): typescript recomendado + react-hooks.
// tsc strict continua responsável por tipos; o CI roda `npm run lint`.
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";

export default tseslint.config(
  {
    ignores: ["dist/", "node_modules/", "src-tauri/"],
  },
  ...tseslint.configs.recommended,
  {
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
    },
  },
);
