export function controlInboxDispositionHealthError(status, body) {
  if (status !== 200) return `expected HTTP 200, got ${status}`;
  if (!body || typeof body !== "object") return "health response is not an object";
  if (body.ok !== true) return "health response is not ok";
  if (
    !body.capabilities ||
    typeof body.capabilities !== "object" ||
    body.capabilities.control_inbox_sender_disposition !== 1
  ) {
    return "control_inbox_sender_disposition capability is not exactly 1";
  }
  return null;
}
