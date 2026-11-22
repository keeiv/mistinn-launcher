// Lint rules for the window's script (src/main.js); CI runs `npm run lint`.
import js from "@eslint/js";
import globals from "globals";

export default [
  js.configs.recommended,
  {
    files: ["src/**/*.js"],
    languageOptions: {
      ecmaVersion: 2024,
      sourceType: "module",
      globals: { ...globals.browser },
    },
    rules: {
      curly: ["error", "multi-line"],
      eqeqeq: ["error", "always"],
      "no-var": "error",
      "prefer-const": "error",
      "no-unused-vars": ["error", { args: "after-used" }],
    },
  },
];
