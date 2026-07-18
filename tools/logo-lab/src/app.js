import { BASE_STATE, PRESETS, buildLogoSvg, cssVariables, logoMetrics, safeRandomState, sanitizeState } from "./model.js";

const controls = ["size", "padding", "stroke", "shield", "glow", "shadow", "wordmarkSpacing"];
let state = sanitizeState(PRESETS.homepage);
let paths = null;
let history = [];
let toastTimer;

function parsePublicMark(markup) {
  const documentNode = new DOMParser().parseFromString(markup, "image/svg+xml");
  if (documentNode.querySelector("parsererror")) throw new Error("The public OSL SVG could not be parsed.");
  return [...documentNode.querySelectorAll("path")].map((path) => ({ d: path.getAttribute("d") ?? "", transform: path.getAttribute("transform") ?? "" }));
}

function showToast(message) {
  const toast = document.querySelector("#toast");
  toast.textContent = message;
  toast.classList.add("visible");
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => toast.classList.remove("visible"), 1800);
}

function pushHistory() {
  history.push(structuredClone(state));
  if (history.length > 60) history.shift();
  document.querySelector("#undo").disabled = history.length === 0;
}

function setState(next, record = true) {
  const clean = sanitizeState(next);
  if (JSON.stringify(clean) === JSON.stringify(state)) return;
  if (record) pushHistory();
  state = clean;
  render();
}

function render() {
  for (const id of controls) {
    const input = document.querySelector(`#${id}`);
    input.value = state[id];
    document.querySelector(`[data-output="${id}"]`).textContent = id === "stroke" ? `${state[id]} px` : id === "shield" ? `${state[id] > 0 ? "+" : ""}${state[id]}` : `${state[id]} px`;
  }
  document.querySelector("#color").value = state.color;
  document.querySelector("#wordmark").checked = state.wordmark;
  document.querySelectorAll("[data-context]").forEach((button) => button.setAttribute("aria-pressed", String(button.dataset.context === state.context)));
  const stage = document.querySelector("#preview-stage");
  stage.className = `preview-stage context-${state.context}`;
  if (!paths) return;
  const svg = buildLogoSvg(state, paths);
  const preview = document.querySelector("#preview");
  preview.classList.add("updating");
  preview.innerHTML = svg;
  requestAnimationFrame(() => preview.classList.remove("updating"));
  const metrics = logoMetrics(state);
  document.querySelector("#dimensions").textContent = `${metrics.width} × ${metrics.height}`;
  document.querySelector("#undo").disabled = history.length === 0;
}

function download(content, name, type) {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = name;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 0);
}

async function exportPng() {
  if (!paths) return;
  const svg = buildLogoSvg(state, paths);
  const metrics = logoMetrics(state);
  const width = Number(document.querySelector("#png-size").value);
  const height = Math.max(1, Math.round(width * metrics.height / metrics.width));
  const url = URL.createObjectURL(new Blob([svg], { type: "image/svg+xml" }));
  try {
    const image = new Image();
    image.decoding = "async";
    image.src = url;
    await image.decode();
    const canvas = document.createElement("canvas");
    canvas.width = width;
    canvas.height = height;
    canvas.getContext("2d", { alpha: true }).drawImage(image, 0, 0, width, height);
    const blob = await new Promise((resolve) => canvas.toBlob(resolve, "image/png"));
    if (!blob) throw new Error("PNG encoding failed");
    const pngUrl = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = pngUrl;
    anchor.download = `osl-logo-${width}px.png`;
    anchor.click();
    setTimeout(() => URL.revokeObjectURL(pngUrl), 0);
    showToast(`PNG exported at ${width} px`);
  } finally {
    URL.revokeObjectURL(url);
  }
}

for (const id of controls) {
  document.querySelector(`#${id}`).addEventListener("input", (event) => setState({ ...state, [id]: Number(event.currentTarget.value) }));
}
document.querySelector("#color").addEventListener("input", (event) => setState({ ...state, color: event.currentTarget.value }));
document.querySelector("#wordmark").addEventListener("change", (event) => setState({ ...state, wordmark: event.currentTarget.checked }));
document.querySelectorAll("[data-context]").forEach((button) => button.addEventListener("click", () => setState({ ...state, context: button.dataset.context })));
document.querySelectorAll("[data-preset]").forEach((button) => button.addEventListener("click", () => setState(PRESETS[button.dataset.preset])));
document.querySelector("#undo").addEventListener("click", () => {
  const previous = history.pop();
  if (!previous) return;
  state = previous;
  render();
});
document.querySelector("#reset").addEventListener("click", () => setState(BASE_STATE));
document.querySelector("#random").addEventListener("click", () => setState(safeRandomState()));
document.querySelector("#export-svg").addEventListener("click", () => { download(buildLogoSvg(state, paths), "osl-logo.svg", "image/svg+xml"); showToast("SVG exported"); });
document.querySelector("#export-css").addEventListener("click", () => { download(cssVariables(state), "osl-logo.css", "text/css"); showToast("CSS variables exported"); });
document.querySelector("#export-png").addEventListener("click", () => void exportPng().catch(() => showToast("PNG export failed")));
window.addEventListener("keydown", (event) => {
  if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "z") { event.preventDefault(); document.querySelector("#undo").click(); }
});

try {
  const response = await fetch("/source/logo-mark.svg", { cache: "no-store" });
  if (!response.ok) throw new Error("The public OSL mark is unavailable.");
  paths = parsePublicMark(await response.text());
  render();
} catch {
  document.querySelector("#preview").textContent = "The public OSL mark could not be loaded.";
  document.querySelectorAll("button, input, select").forEach((control) => { control.disabled = true; });
}
