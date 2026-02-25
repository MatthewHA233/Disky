export interface DirEntry {
  name: string;
  path: string;
  files: number;
  subdirs: number;
  logical_size: number;
  is_dir: boolean;
}

export interface DriveInfo {
  mount_point: string;
  total_space: number;
  available_space: number;
}

export interface ScanProgress {
  status: "idle" | "scanning" | "done" | "error";
  message: string;
}

export interface ScanProgressEvent {
  files_scanned: number;
  dirs_scanned: number;
  total_size: number;
}

export interface ItemInfo {
  path: string;
  size: number;
  is_dir: boolean;
}

export interface DeleteResult {
  path: string;
  success: boolean;
  error: string | null;
}

export interface ScanRecord {
  id: number;
  drive: string;
  entry_count: number;
  created_at: string;
}

export interface DiffEntry {
  path: string;
  name: string;
  old_size: number;
  new_size: number;
  diff: number;
}

export interface ScanTreeEvent {
  children: DirEntry[];
  scanning: boolean;
  current_dir: string;
}
