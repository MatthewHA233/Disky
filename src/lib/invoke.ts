import { invoke } from "@tauri-apps/api/core";
import type { ChildrenPage, DirEntry, DriveInfo, Footprint, ItemInfo, DeleteResult, ScanRecord, DiffEntry, LoadScanResult, AiSettings, ChatMessage, ChatSession, CliDetection, Note, AiAnalysis, AnalyzePathInput, EmptyFolderResult, MoveResult, Tag, FileTag, TaggedPath, CleanupLogPage, ArchiveModel, ArchiveBoxRow, ArchiveAiPlacement, ArchiveClassifyItem, ArchiveCategory, ArchiveCacheCheck, ArchiveCheckItem, ClaimKind, ArchiveAttributeResult, ArchiveMode, ArchiveNote, ArchiveLooseList, SnapshotInfo, LoadSnapshotResult } from "../types";

export const listDrives = () => invoke<DriveInfo[]>("list_drives");

export const scanDisk = (path: string) => invoke<number>("scan_disk", { path });

export const getChildren = (parentPath: string, topN: number) =>
  invoke<DirEntry[]>("get_children", { parentPath, topN });

export const getChildrenPage = (parentPath: string, offset: number, limit: number) =>
  invoke<ChildrenPage>("get_children_page", { parentPath, offset, limit });

export const classifyFootprints = () => invoke<Footprint[]>("classify_footprints");

export const getItemsInfo = (paths: string[]) => invoke<ItemInfo[]>("get_items_info", { paths });

export const deleteItems = (paths: string[], toTrash: boolean) =>
  invoke<DeleteResult[]>("delete_items", { paths, toTrash });

export const saveScan = (drive: string, cleanedBytes: number) =>
  invoke<number>("save_scan", { drive, cleanedBytes });

export const loadScan = (id: number) => invoke<LoadScanResult>("load_scan", { id });

export const deleteScan = (id: number) => invoke<void>("delete_scan", { id });

export const listScans = () => invoke<ScanRecord[]>("list_scans");

/** 各盘最近一次整盘扫描存下来的结果 */
export const listSnapshots = () => invoke<SnapshotInfo[]>("list_snapshots");
/** 打开某块盘存下来的扫描，当作当前的扫描结果 */
export const loadSnapshot = (root: string) => invoke<LoadSnapshotResult>("load_snapshot", { root });
/** 删除过东西以后，按内存里的树重存一遍 */
export const saveSnapshot = () => invoke<void>("save_snapshot");

export const compareScans = (idA: number, idB: number, root: string) =>
  invoke<DiffEntry[]>("compare_scans", { idA, idB, root });

// Cleanup logs
export const saveCleanupLog = (drive: string, path: string, actionType: string, bytesFreed: number, sessionTs: string, description: string) =>
  invoke<number>("save_cleanup_log", { drive, path, actionType, bytesFreed, sessionTs, description });
export const listCleanupLogs = (page: number, pageSize: number) =>
  invoke<CleanupLogPage>("list_cleanup_logs", { page, pageSize });

// AI
export const loadAiSettings = () => invoke<AiSettings>("load_ai_settings");
export const saveAiSettings = (settings: AiSettings) =>
  invoke<void>("save_ai_settings", { settings });
export const sendChatMessage = (message: string, sessionId: number) =>
  invoke<void>("send_chat_message", { message, sessionId });
export const listChatMessages = (sessionId: number) =>
  invoke<ChatMessage[]>("list_chat_messages", { sessionId });
export const detectClaudeCli = () => invoke<CliDetection>("detect_claude_cli");

// Chat sessions
export const listChatSessions = () => invoke<ChatSession[]>("list_chat_sessions");
export const createChatSession = () => invoke<ChatSession>("create_chat_session");
export const renameChatSession = (sessionId: number, title: string) =>
  invoke<void>("rename_chat_session", { sessionId, title });
export const deleteChatSession = (sessionId: number) =>
  invoke<void>("delete_chat_session", { sessionId });
export const regenerateChatTitle = (sessionId: number) =>
  invoke<void>("regenerate_chat_title", { sessionId });

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
export const removeScanNode = (path: string) =>
  invoke<void>("remove_scan_node", { path });

// Tags
export const listTags = () => invoke<Tag[]>("list_tags");
export const createTag = (name: string, color: string) =>
  invoke<Tag>("create_tag", { name, color });
export const renameTag = (id: number, name: string) =>
  invoke<void>("rename_tag", { id, name });
export const deleteTag = (id: number) => invoke<void>("delete_tag", { id });
export const toggleTag = (path: string, tagId: number, isDir: boolean) =>
  invoke<boolean>("toggle_tag", { path, tagId, isDir });
export const getTagsForPaths = (paths: string[]) =>
  invoke<FileTag[]>("get_tags_for_paths", { paths });
export const listTaggedPaths = (tagId?: number) =>
  invoke<TaggedPath[]>("list_tagged_paths", { tagId: tagId ?? null });

// File system watcher
export const startWatch = (root: string) => invoke<void>("start_watch", { root });
export const stopWatch = () => invoke<void>("stop_watch");
export const setWatchPath = (path: string) => invoke<void>("set_watch_path", { path });

// macOS 完全磁盘访问权限（Windows 上 check 恒为 true，open 为 no-op）
export const checkFullDiskAccess = () => invoke<boolean>("check_full_disk_access");
export const openFullDiskAccessSettings = () => invoke<void>("open_full_disk_access_settings");

// Archive
export const getArchive = () => invoke<ArchiveModel>("get_archive");
export const archivePlace = (keys: string[], boxId: string | null) =>
  invoke<void>("archive_place", { keys, boxId });
/** 指定某个位置属于哪个软件的哪种成分；appKey 为 null 表示不属于任何软件 */
export const archiveAssign = (path: string, appKey: string | null, kind: ClaimKind) =>
  invoke<void>("archive_assign", { path, appKey, kind });
/** 撤掉指定的归属，回到内置规则的判断 */
export const archiveUnassign = (path: string) => invoke<void>("archive_unassign", { path });
/** 整理模式；还没走完新手引导时为 null */
export const archiveMode = () => invoke<ArchiveMode | null>("archive_mode");
export const archiveSetMode = (mode: ArchiveMode) => invoke<void>("archive_set_mode", { mode });
/** 重新开始整理（先备份数据库），返回备份文件的位置 */
export const archiveReset = () => invoke<string>("archive_reset");
export const archiveNotes = () => invoke<ArchiveNote[]>("archive_notes");
export const archiveDeleteNote = (id: number) => invoke<void>("archive_delete_note", { id });
/** 用还没保存的设置试一次连接，返回 AI 的回复 */
export const testAiSettings = (settings: AiSettings) => invoke<string>("test_ai_settings", { settings });
/** 让 AI 用内置工具查这些文件夹属于哪个软件；进度通过 archive-agent 事件推送 */
export const archiveAiAttribute = (paths: string[]) => invoke<ArchiveAttributeResult>("archive_ai_attribute", { paths });
export const archiveCreateBox = (name: string, color: string, category: ArchiveCategory) =>
  invoke<ArchiveBoxRow>("archive_create_box", { name, color, category });
export const archiveRenameBox = (id: string, name: string) => invoke<void>("archive_rename_box", { id, name });
export const archiveDeleteBox = (id: string) => invoke<void>("archive_delete_box", { id });
export const archiveIcons = (sources: string[]) => invoke<(string | null)[]>("archive_icons", { sources });
export const archiveForget = (items: [string, number][]) => invoke<void>("archive_forget", { items });
/** 现场列一个文件夹里没单独列出的东西；skip / known 见 looseOf */
export const archiveLoose = (path: string, skip: string[], known: [string, number][]) =>
  invoke<ArchiveLooseList>("archive_loose", { path, skip, known });
export const archiveAiClassify = (items: ArchiveClassifyItem[], category: ArchiveCategory) =>
  invoke<ArchiveAiPlacement[]>("archive_ai_classify", { items, category });
export const archiveAiCheck = (items: ArchiveCheckItem[]) => invoke<ArchiveCacheCheck[]>("archive_ai_check", { items });
