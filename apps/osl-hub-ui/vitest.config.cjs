const discordQaRendererDefine = (mode) => mode === "discord-qa"
  ? { "import.meta.env.VITE_OSL_DISCORD_QA_SHELL": JSON.stringify("1") }
  : { "import.meta.env.VITE_OSL_DISCORD_QA_SHELL": JSON.stringify("0") };

module.exports = ({ mode }) => ({
  define: {
    ...discordQaRendererDefine(mode),
    "import.meta.env.VITE_OSL_SIGNAL_QA_SHELL": JSON.stringify(mode === "signal-qa" ? "1" : "0"),
  },
  test: {
    exclude: ["**/node_modules/**", "**/dist/**", "screenshots/**"],
  },
});
