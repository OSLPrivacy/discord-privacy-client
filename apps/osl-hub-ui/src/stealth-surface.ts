export const DECOY_DOCUMENT_TITLE = "Decoy workspace";

/**
 * Replace the product document name before the decoy is painted. The trusted
 * desktop command independently clears the native window title, leaving this
 * as the decoy screen's one accessible identity.
 */
export function enterDecoyDocument(documentLike: Pick<Document, "title">): void {
  documentLike.title = DECOY_DOCUMENT_TITLE;
}
