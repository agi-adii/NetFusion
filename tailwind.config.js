/** @type {import('tailwindcss').Config} */
export default {
  darkMode: "class",
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      fontFamily: {
        sans: ['"Segoe UI Variable Display"', '"Segoe UI"', "Inter", "system-ui", "sans-serif"],
        mono: ['"Cascadia Code"', '"JetBrains Mono"', "Consolas", "monospace"],
      },
      colors: {
        navy: {
          950: "#050814",
          900: "#090e21",
          850: "#0e1530",
          800: "#141d3f",
          700: "#1e2a58",
        },
        electric: {
          400: "#38bdf8",
          500: "#0ea5e9",
          600: "#0284c7",
        },
        lavender: {
          300: "#c4b5fd",
          400: "#a78bfa",
          500: "#8b5cf6",
          600: "#7c3aed",
        },
      },
      boxShadow: {
        glass: "0 8px 32px 0 rgba(3, 7, 24, 0.55)",
        "glow-blue": "0 0 24px -4px rgba(56, 189, 248, 0.45)",
        "glow-lavender": "0 0 24px -4px rgba(167, 139, 250, 0.45)",
      },
    },
  },
  plugins: [],
};

