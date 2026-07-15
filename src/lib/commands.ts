import { invoke } from "@tauri-apps/api/core";

export interface CommandErrorPayload {
  code: string;
  message: string;
  retryable: boolean;
  incident_id: string;
}

export interface CommandWarningPayload {
  code: string;
  step: string;
  message: string;
  incident_id: string;
}

export interface ActionReceipt {
  action: string;
  target: string;
  affected: number;
}

type CommandLanguage = "en-US" | "zh-CN" | "zh-TW" | "fr-FR" | "ru-RU";

let commandLanguage: CommandLanguage = detectLanguage();

const errorMessages: Record<CommandLanguage, Record<string, string>> = {
  "en-US": {
    auto_clean_already_running: "Automatic cleanup is already running.",
    auto_clean_unavailable: "Automatic cleanup could not be completed.",
    config_persistence_failed: "The application settings could not be saved.",
    database_unavailable: "The cleanup database is unavailable.",
    internal_unexpected: "An unexpected application error occurred.",
    privacy_write_blocked:
      "The operation is unavailable while privacy mode is active.",
    quick_access_already_exists: "The item is already present in Quick Access.",
    quick_access_metadata_unavailable:
      "Quick Access metadata is unavailable or unsupported.",
    quick_access_operation_failed:
      "The Quick Access operation could not be completed.",
    quick_access_partial_failure: "The Quick Access cleanup partially completed.",
    quick_access_permission_denied:
      "Windows or PowerShell permissions blocked the Quick Access operation.",
    quick_access_post_mutation_failed:
      "The Quick Access change completed, but a follow-up step failed.",
    quick_access_timeout: "The Quick Access operation timed out.",
    resource_not_found: "The requested resource was not found.",
    system_operation_failed: "The Windows operation could not be completed.",
    validation_invalid_argument: "The request contains an invalid value.",
  },
  "zh-CN": {
    auto_clean_already_running: "自动清理正在运行。",
    auto_clean_unavailable: "无法完成自动清理。",
    config_persistence_failed: "无法保存应用设置。",
    database_unavailable: "清理数据库不可用。",
    internal_unexpected: "应用发生意外错误。",
    privacy_write_blocked: "隐私模式启用时无法执行此操作。",
    quick_access_already_exists: "项目已存在于快速访问中。",
    quick_access_metadata_unavailable: "快速访问元数据不可用或不受支持。",
    quick_access_operation_failed: "无法完成快速访问操作。",
    quick_access_partial_failure: "快速访问清理已部分完成。",
    quick_access_permission_denied: "Windows 或 PowerShell 权限阻止了快速访问操作。",
    quick_access_post_mutation_failed: "快速访问变更已完成，但后续步骤失败。",
    quick_access_timeout: "快速访问操作超时。",
    resource_not_found: "未找到请求的资源。",
    system_operation_failed: "无法完成 Windows 操作。",
    validation_invalid_argument: "请求中包含无效值。",
  },
  "zh-TW": {
    auto_clean_already_running: "自動清理正在執行。",
    auto_clean_unavailable: "無法完成自動清理。",
    config_persistence_failed: "無法儲存應用程式設定。",
    database_unavailable: "清理資料庫無法使用。",
    internal_unexpected: "應用程式發生未預期的錯誤。",
    privacy_write_blocked: "隱私模式啟用時無法執行此操作。",
    quick_access_already_exists: "項目已存在於快速存取中。",
    quick_access_metadata_unavailable: "快速存取中繼資料不可用或不受支援。",
    quick_access_operation_failed: "無法完成快速存取操作。",
    quick_access_partial_failure: "快速存取清理已部分完成。",
    quick_access_permission_denied: "Windows 或 PowerShell 權限阻止了快速存取操作。",
    quick_access_post_mutation_failed: "快速存取變更已完成，但後續步驟失敗。",
    quick_access_timeout: "快速存取操作逾時。",
    resource_not_found: "找不到要求的資源。",
    system_operation_failed: "無法完成 Windows 操作。",
    validation_invalid_argument: "要求中包含無效值。",
  },
  "fr-FR": {
    auto_clean_already_running: "Le nettoyage automatique est déjà en cours.",
    auto_clean_unavailable: "Le nettoyage automatique n’a pas pu être effectué.",
    config_persistence_failed: "Les paramètres de l’application n’ont pas pu être enregistrés.",
    database_unavailable: "La base de données de nettoyage est indisponible.",
    internal_unexpected: "Une erreur inattendue de l’application s’est produite.",
    privacy_write_blocked: "Cette opération est indisponible lorsque le mode privé est actif.",
    quick_access_already_exists: "L’élément est déjà présent dans l’accès rapide.",
    quick_access_metadata_unavailable: "Les métadonnées de l’accès rapide sont indisponibles ou non prises en charge.",
    quick_access_operation_failed: "L’opération d’accès rapide n’a pas pu être effectuée.",
    quick_access_partial_failure: "Le nettoyage de l’accès rapide est partiellement terminé.",
    quick_access_permission_denied: "Les autorisations Windows ou PowerShell ont bloqué l’opération.",
    quick_access_post_mutation_failed: "La modification est terminée, mais une étape de suivi a échoué.",
    quick_access_timeout: "L’opération d’accès rapide a expiré.",
    resource_not_found: "La ressource demandée est introuvable.",
    system_operation_failed: "L’opération Windows n’a pas pu être effectuée.",
    validation_invalid_argument: "La requête contient une valeur non valide.",
  },
  "ru-RU": {
    auto_clean_already_running: "Автоматическая очистка уже выполняется.",
    auto_clean_unavailable: "Не удалось выполнить автоматическую очистку.",
    config_persistence_failed: "Не удалось сохранить настройки приложения.",
    database_unavailable: "База данных очистки недоступна.",
    internal_unexpected: "Произошла непредвиденная ошибка приложения.",
    privacy_write_blocked: "Операция недоступна, пока включён приватный режим.",
    quick_access_already_exists: "Элемент уже находится в быстром доступе.",
    quick_access_metadata_unavailable: "Метаданные быстрого доступа недоступны или не поддерживаются.",
    quick_access_operation_failed: "Не удалось выполнить операцию быстрого доступа.",
    quick_access_partial_failure: "Очистка быстрого доступа выполнена частично.",
    quick_access_permission_denied: "Разрешения Windows или PowerShell заблокировали операцию.",
    quick_access_post_mutation_failed: "Изменение завершено, но последующий шаг завершился ошибкой.",
    quick_access_timeout: "Операция быстрого доступа превысила время ожидания.",
    resource_not_found: "Запрошенный ресурс не найден.",
    system_operation_failed: "Не удалось выполнить операцию Windows.",
    validation_invalid_argument: "Запрос содержит недопустимое значение.",
  },
};

export class CommandInvocationError extends Error {
  readonly code: string;
  readonly retryable: boolean;
  readonly incidentId: string;

  constructor(payload: CommandErrorPayload) {
    super(commandIssueMessage(payload));
    this.name = "CommandInvocationError";
    this.code = payload.code;
    this.retryable = payload.retryable;
    this.incidentId = payload.incident_id;
  }
}

export function commandIssueMessage(
  payload: Pick<CommandErrorPayload, "code" | "message" | "incident_id">,
) {
  const message =
    errorMessages[commandLanguage][payload.code] ??
    payload.message;
  return `${message} (${payload.incident_id})`;
}

export function setCommandErrorLanguage(language: string) {
  if (language in errorMessages) {
    commandLanguage = language as CommandLanguage;
  }
}

export async function invokeCommand<T>(
  command: string,
  args?: Record<string, unknown>,
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw toCommandError(error);
  }
}

export function commandErrorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function toCommandError(error: unknown): Error {
  if (isCommandErrorPayload(error)) {
    return new CommandInvocationError(error);
  }
  if (typeof error === "string") {
    return new Error(error);
  }
  return new Error("The application command could not be completed.");
}

function isCommandErrorPayload(error: unknown): error is CommandErrorPayload {
  if (!error || typeof error !== "object") {
    return false;
  }
  const value = error as Partial<CommandErrorPayload>;
  return (
    typeof value.code === "string" &&
    typeof value.message === "string" &&
    typeof value.retryable === "boolean" &&
    typeof value.incident_id === "string"
  );
}

function detectLanguage(): CommandLanguage {
  const language = typeof navigator === "undefined" ? "en-US" : navigator.language;
  if (language.toLowerCase().startsWith("zh")) {
    return /tw|hk|mo|hant/i.test(language) ? "zh-TW" : "zh-CN";
  }
  if (language.toLowerCase().startsWith("fr")) {
    return "fr-FR";
  }
  if (language.toLowerCase().startsWith("ru")) {
    return "ru-RU";
  }
  return "en-US";
}
