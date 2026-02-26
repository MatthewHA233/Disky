import { invoke } from "@tauri-apps/api/core";
import type { DirEntry, DriveInfo, ItemInfo, DeleteResult, ScanRecord, DiffEntry, LoadScanResult, AiSettings, ChatMessage, Note, AiAnalysis, AnalyzePathInput } from "../types";

export const listDrives = () => invoke<DriveInfo[]>("list_drives");

export const scanDisk = (path: string) => invoke<number>("scan_disk", { path });

export const getChildren = (parentPath: string, topN: number) =>
  invoke<DirEntry[]>("get_children", { parentPath, topN });

export const getItemsInfo = (paths: string[]) => invoke<ItemInfo[]>("get_items_info", { paths });

export const deleteItems = (paths: string[], toTrash: boolean) =>
  invoke<DeleteResult[]>("delete_items", { paths, toTrash });

export const saveScan = (drive: string) => invoke<number>("save_scan", { drive });

export const loadScan = (id: number) => invoke<LoadScanResult>("load_scan", { id });

export const deleteScan = (id: number) => invoke<void>("delete_scan", { id });

export const listScans = () => invoke<ScanRecord[]>("list_scans");

export const compareScans = (idA: number, idB: number, root: string) =>
  invoke<DiffEntry[]>("compare_scans", { idA, idB, root });

// AI
export const loadAiSettings = () => invoke<AiSettings>("load_ai_settings");
export const saveAiSettings = (settings: AiSettings) =>
  invoke<void>("save_ai_settings", { settings });
export const sendChatMessage = (message: string) =>
  invoke<void>("send_chat_message", { message });
export const listChatMessages = () => invoke<ChatMessage[]>("list_chat_messages");
export const clearChatHistory = () => invoke<void>("clear_chat_history");

// AI Analysis
export const analyzePaths = (items: AnalyzePathInput[]) =>
  invoke<{ path: string; description: string; priority: number }[]>("analyze_paths", { items });
export const saveAiAnalysis = (path: string, description: string, priority: number) =>
  invoke<void>("save_ai_analysis", { path, description, priority });
export const loadAiAnalyses = (paths: string[]) =>
  invoke<AiAnalysis[]>("load_ai_analyses", { paths });
export const loadAllAiAnalyses = () => invoke<AiAnalysis[]>("load_all_ai_analyses");

// Notes
export const saveNote = (path: string, content: string) =>
  invoke<number>("save_note", { path, content });
export const getNotesForPath = (path: string) =>
  invoke<Note[]>("get_notes_for_path", { path });
export const deleteNote = (id: number) => invoke<void>("delete_note", { id });
export const listAllNotes = () => invoke<Note[]>("list_all_notes");
