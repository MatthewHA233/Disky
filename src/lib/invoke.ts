import { invoke } from "@tauri-apps/api/core";
import type { DirEntry, DriveInfo, ItemInfo, DeleteResult, ScanRecord, DiffEntry } from "../types";

export const listDrives = () => invoke<DriveInfo[]>("list_drives");

export const scanDisk = (path: string) => invoke<number>("scan_disk", { path });

export const getChildren = (parentPath: string, topN: number) =>
  invoke<DirEntry[]>("get_children", { parentPath, topN });

export const getItemsInfo = (paths: string[]) => invoke<ItemInfo[]>("get_items_info", { paths });

export const deleteItems = (paths: string[], toTrash: boolean) =>
  invoke<DeleteResult[]>("delete_items", { paths, toTrash });

export const saveScan = (drive: string) => invoke<number>("save_scan", { drive });

export const listScans = () => invoke<ScanRecord[]>("list_scans");

export const compareScans = (idA: number, idB: number, root: string) =>
  invoke<DiffEntry[]>("compare_scans", { idA, idB, root });
