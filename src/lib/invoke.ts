import { invoke } from "@tauri-apps/api/core";
import type { DirEntry, DriveInfo, ItemInfo, DeleteResult, ScanRecord, DiffEntry, LoadScanResult, AiSettings, ChatMessage, Note, AiAnalysis, AnalyzePathInput, EmptyFolderResult, MoveResult, Tag, FileTag, TaggedPath } from "../types";

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

// Context Menu
export const openPath = (path: string) =>
  invoke<void>("open_path", { path });
export const showInExplorer = (path: string) =>
  invoke<void>("show_in_explorer", { path });
export const openInTerminal = (path: string, shell: string) =>
  invoke<void>("open_in_terminal", { path, shell });
export const showProperties = (path: string) =>
  invoke<void>("show_properties", { path });
export const emptyFolder = (path: string, toTrash: boolean) =>
  invoke<EmptyFolderResult>("empty_folder", { path, toTrash });
export const pickFolderAndMove = (path: string) =>
  invoke<MoveResult>("pick_folder_and_move", { path });
export const refreshScanNode = (path: string) =>
  invoke<DirEntry[]>("refresh_scan_node", { path });

// Tags
export const listTags = () => invoke<Tag[]>("list_tags");
export const createTag = (name: string, color: string) =>
  invoke<Tag>("create_tag", { name, color });
export const renameTag = (id: number, name: string) =>
  invoke<void>("rename_tag", { id, name });
export const deleteTag = (id: number) => invoke<void>("delete_tag", { id });
export const toggleTag = (path: string, tagId: number) =>
  invoke<boolean>("toggle_tag", { path, tagId });
export const getTagsForPaths = (paths: string[]) =>
  invoke<FileTag[]>("get_tags_for_paths", { paths });
export const listTaggedPaths = (tagId?: number) =>
  invoke<TaggedPath[]>("list_tagged_paths", { tagId: tagId ?? null });
