const { readFileSync } = require("node:fs");
const { resolve } = require("node:path");

const releaseScopeNoteId = "virtual:release-scope-note";
const resolvedReleaseScopeNoteId = `\0${releaseScopeNoteId}`;
const releaseScopeNotePath = resolve(__dirname, "../../docs/release/release-scope-exclusions.md");

const discordQaRendererDefine = (mode) => mode === "discord-qa"
  ? { "import.meta.env.VITE_OSL_DISCORD_QA_SHELL": JSON.stringify("1") }
  : { "import.meta.env.VITE_OSL_DISCORD_QA_SHELL": JSON.stringify("0") };

module.exports = ({ mode }) => ({
  plugins: [{
    name: "release-scope-note",
    resolveId(id) {
      return id === releaseScopeNoteId ? resolvedReleaseScopeNoteId : null;
    },
    load(id) {
      if (id !== resolvedReleaseScopeNoteId) return null;
      return `export default ${JSON.stringify(readFileSync(releaseScopeNotePath, "utf8"))};`;
    },
  }],
  define: {
    ...discordQaRendererDefine(mode),
    "import.meta.env.VITE_OSL_SIGNAL_QA_SHELL": JSON.stringify(mode === "signal-qa" ? "1" : "0"),
  },
  test: {
    exclude: ["**/node_modules/**", "**/dist/**", "screenshots/**"],
  },
});
