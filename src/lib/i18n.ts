// Interface text. Rust picks the language from Windows; `setLocale` runs once, before mounting.
import type { Locale } from "./ipc";

const en = {
  "app.pin": "Pin panel",
  "notes.label": "Notes",
  "notes.search": "Search or create a note",
  "notes.searchPlaceholder": "Search notes or enter a URL…",
  "notes.changeFolder": "Change folder: {path}",
  "notes.emptyFolder":
    "No notes yet. Type a title in the bar above and press Ctrl+Enter to create one.",
  "notes.unavailable": "The notes folder is not available.",
  "notes.chooseHint": "Choose a folder to browse your notes.",
  "notes.choose": "Choose folder",
  "notes.gone": "This note was moved or deleted outside the app.",
  "list.pinned": "Pinned",
  "list.notes": "Notes",
  "list.create": "Create “{text}”",
  "card.pin": "Pin (Ctrl+D)",
  "card.unpin": "Unpin (Ctrl+D)",
  "card.show": "Open (→)",
  "card.showAll": "Show all",
  "card.showLess": "Show less",
  "card.collapse": "Close (←)",
  "card.close": "Close note",
  "card.missing": "Note not found",
  "card.remove": "Remove from pinned (Delete)",
  "pinned.deleted": "The pinned list was deleted.",
  "pinned.restore": "Restore pinned items",
  "editor.updated": "Updated from disk",
  "editor.undo": "Undo",
  "editor.deleted": "Deleted outside the app; your next edit recreates it.",
  "editor.toggle": "Edit (Ctrl+E)",
  "editor.saving": "Saving…",
  "editor.saved": "Saved",
  "editor.label": "Note text",
  "editor.conflict": "This note changed outside the app. Your edits are kept here.",
  "editor.keep": "Keep my version",
  "editor.discard": "Discard my changes",
  "leftovers.label": "Interrupted saves",
  "leftovers.notice":
    "Interrupted saves left these copies. Restoring turns each one into a note; nothing is deleted.",
  "leftovers.restore": "Restore {note}",
  "web.openAddress": "Open {text}",
  "web.search": "Search the web: {text}",
  "web.address": "Page address",
  "web.page": "Return to page",
  "web.notes": "Notes",
  "web.back": "Back",
  "web.forward": "Forward",
  "error.unexpected": "Something went wrong: {detail}",
  "error.noFolder": "Choose a notes folder.",
  "error.folderUnavailable": "The notes folder is not available.",
  "error.invalidPath": "Invalid file path.",
  "error.outsideFolder": "The file is outside the chosen folder.",
  "error.tooLarge": "The note has {sizeMb} MB; the limit is {limitMb} MB.",
  "error.notUtf8": "The note is not UTF-8 text.",
  "error.unsupportedImage": "Paste a PNG, JPEG, GIF or WebP image.",
  "error.imageTooLarge": "The image exceeds the {limitMb} MB limit.",
  "error.attachmentFolderUnsafe":
    "Cannot save an attachment through a folder link or an unavailable folder.",
  "error.saveFailed": "Could not save the note: {detail}",
  "error.previousKept":
    "Could not save the note; the previous version was kept at {path}: {detail}",
  "error.externalKept":
    "The note changed outside the app; the external version was kept at {path}: {detail}",
  "error.externalAndOursKept":
    "The note changed outside the app; the external version was kept at {external} and your text at {ours}: {detail}",
  "error.oursKept":
    "The note changed outside the app while saving; a copy of your text was kept at {path}.",
  "error.pinnedChanged":
    "The pinned list changed outside the app while it was being updated; try again.",
  "error.invalidUrl": "Invalid address: {detail}",
  "error.urlNotAllowed": "Address not allowed: {url}",
  "error.linkNotAllowed": "Only web and email links open outside the app.",
};

export type MessageKey = keyof typeof en;

const ptBR: Record<MessageKey, string> = {
  "app.pin": "Fixar painel",
  "notes.label": "Notas",
  "notes.search": "Buscar ou criar uma nota",
  "notes.searchPlaceholder": "Buscar notas ou digitar URL…",
  "notes.changeFolder": "Trocar pasta: {path}",
  "notes.emptyFolder":
    "Nenhuma nota ainda. Digite um título na barra acima e tecle Ctrl+Enter para criar uma.",
  "notes.unavailable": "A pasta de notas não está disponível.",
  "notes.chooseHint": "Escolha uma pasta para consultar suas notas.",
  "notes.choose": "Escolher pasta",
  "notes.gone": "Esta nota foi movida ou apagada fora do app.",
  "list.pinned": "Fixados",
  "list.notes": "Notas",
  "list.create": "Criar “{text}”",
  "card.pin": "Fixar (Ctrl+D)",
  "card.unpin": "Desafixar (Ctrl+D)",
  "card.show": "Abrir (→)",
  "card.showAll": "Mostrar tudo",
  "card.showLess": "Mostrar menos",
  "card.collapse": "Fechar (←)",
  "card.close": "Fechar nota",
  "card.missing": "Nota não encontrada",
  "card.remove": "Remover dos fixados (Delete)",
  "pinned.deleted": "A lista de fixados foi apagada.",
  "pinned.restore": "Restaurar fixados",
  "editor.updated": "Atualizada do disco",
  "editor.undo": "Desfazer",
  "editor.deleted": "Apagada fora do app; sua próxima edição a recria.",
  "editor.toggle": "Editar (Ctrl+E)",
  "editor.saving": "Salvando…",
  "editor.saved": "Salvo",
  "editor.label": "Texto da nota",
  "editor.conflict": "Esta nota mudou fora do app. Suas edições estão mantidas aqui.",
  "editor.keep": "Manter minha versão",
  "editor.discard": "Descartar minhas alterações",
  "leftovers.label": "Salvamentos interrompidos",
  "leftovers.notice":
    "Salvamentos interrompidos deixaram estas cópias. Restaurar transforma cada uma em nota; nada é apagado.",
  "leftovers.restore": "Restaurar {note}",
  "web.openAddress": "Abrir {text}",
  "web.search": "Buscar na web: {text}",
  "web.address": "Endereço da página",
  "web.page": "Voltar à página",
  "web.notes": "Notas",
  "web.back": "Voltar",
  "web.forward": "Avançar",
  "error.unexpected": "Algo deu errado: {detail}",
  "error.noFolder": "Escolha uma pasta de notas.",
  "error.folderUnavailable": "A pasta de notas não está disponível.",
  "error.invalidPath": "Caminho de arquivo inválido.",
  "error.outsideFolder": "O arquivo está fora da pasta escolhida.",
  "error.tooLarge": "A nota tem {sizeMb} MB; o limite é {limitMb} MB.",
  "error.notUtf8": "A nota não é texto UTF-8.",
  "error.unsupportedImage": "Cole uma imagem PNG, JPEG, GIF ou WebP.",
  "error.imageTooLarge": "A imagem excede o limite de {limitMb} MB.",
  "error.attachmentFolderUnsafe":
    "Não é possível salvar um anexo por um link de pasta ou uma pasta indisponível.",
  "error.saveFailed": "Não foi possível salvar a nota: {detail}",
  "error.previousKept":
    "Não foi possível salvar a nota; a versão anterior ficou em {path}: {detail}",
  "error.externalKept": "A nota mudou fora do app; a versão externa ficou em {path}: {detail}",
  "error.externalAndOursKept":
    "A nota mudou fora do app; a versão externa ficou em {external} e o seu texto em {ours}: {detail}",
  "error.oursKept":
    "A nota mudou fora do app durante o salvamento; uma cópia do seu texto ficou em {path}.",
  "error.pinnedChanged":
    "A lista de fixados mudou fora do app durante a atualização; tente de novo.",
  "error.invalidUrl": "Endereço inválido: {detail}",
  "error.urlNotAllowed": "Endereço não permitido: {url}",
  "error.linkNotAllowed": "Só links da web e de e-mail abrem fora do app.",
};

const messages: Record<Locale, Record<MessageKey, string>> = { en, "pt-BR": ptBR };
let current: Record<MessageKey, string> = en;

export function setLocale(locale: Locale) {
  current = messages[locale] ?? en;
  document.documentElement.lang = locale;
}

type Params = Record<string, unknown>;

export function t(key: MessageKey, params: Params = {}): string {
  return current[key].replace(/\{(\w+)\}/g, (match, name: string) =>
    name in params ? String(params[name]) : match,
  );
}

/** Text for a failed command: Rust errors arrive as `{ code, ...fields }`. */
export function errorText(error: unknown): string {
  if (typeof error === "object" && error !== null && "code" in error) {
    const key = `error.${String(error.code)}`;
    if (key in en) return t(key as MessageKey, error as Params);
    return t("error.unexpected", { detail: JSON.stringify(error) });
  }
  return t("error.unexpected", { detail: String(error) });
}

export function date(ms: number): string {
  return new Date(ms).toLocaleDateString(document.documentElement.lang || "en", {
    dateStyle: "short",
  });
}

export function dateTime(ms: number): string {
  return new Date(ms).toLocaleString(document.documentElement.lang || "en", {
    dateStyle: "short",
    timeStyle: "short",
  });
}
