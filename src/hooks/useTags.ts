import { useCallback, useEffect, useState } from "react";
import type { Tag, FileTag, TaggedPath } from "../types";
import {
  listTags,
  createTag as invokeCreateTag,
  renameTag as invokeRenameTag,
  deleteTag as invokeDeleteTag,
  toggleTag as invokeToggleTag,
  getTagsForPaths,
  listTaggedPaths as invokeListTaggedPaths,
} from "../lib/invoke";

export interface UseTagsReturn {
  tags: Tag[];
  pathTags: Map<string, FileTag[]>;
  taggedPaths: TaggedPath[];
  loadTags: () => Promise<void>;
  createTag: (name: string, color: string) => Promise<Tag>;
  renameTag: (id: number, name: string) => Promise<void>;
  deleteTag: (id: number) => Promise<void>;
  toggleTag: (path: string, tagId: number) => Promise<boolean>;
  loadTagsForPaths: (paths: string[]) => Promise<void>;
  loadTaggedPaths: (tagId?: number) => Promise<void>;
}

export function useTags(): UseTagsReturn {
  const [tags, setTags] = useState<Tag[]>([]);
  const [pathTags, setPathTags] = useState<Map<string, FileTag[]>>(new Map());
  const [taggedPaths, setTaggedPaths] = useState<TaggedPath[]>([]);

  const loadTags = useCallback(async () => {
    const result = await listTags();
    setTags(result);
  }, []);

  // Load tags on mount
  useEffect(() => {
    loadTags().catch(() => {});
  }, [loadTags]);

  const createTag = useCallback(
    async (name: string, color: string): Promise<Tag> => {
      const tag = await invokeCreateTag(name, color);
      await loadTags();
      return tag;
    },
    [loadTags],
  );

  const renameTag = useCallback(
    async (id: number, name: string) => {
      await invokeRenameTag(id, name);
      await loadTags();
    },
    [loadTags],
  );

  const deleteTag = useCallback(
    async (id: number) => {
      await invokeDeleteTag(id);
      await loadTags();
      // Remove from pathTags cache
      setPathTags((prev) => {
        const next = new Map<string, FileTag[]>();
        for (const [path, fts] of prev) {
          const filtered = fts.filter((ft) => ft.tag_id !== id);
          if (filtered.length > 0) next.set(path, filtered);
        }
        return next;
      });
    },
    [loadTags],
  );

  const toggleTag = useCallback(
    async (path: string, tagId: number): Promise<boolean> => {
      const added = await invokeToggleTag(path, tagId);
      // Refresh tags for this path
      const updated = await getTagsForPaths([path]);
      setPathTags((prev) => {
        const next = new Map(prev);
        if (updated.length > 0) {
          next.set(path, updated);
        } else {
          next.delete(path);
        }
        return next;
      });
      return added;
    },
    [],
  );

  const loadTagsForPaths = useCallback(async (paths: string[]) => {
    if (paths.length === 0) return;
    const result = await getTagsForPaths(paths);
    const grouped = new Map<string, FileTag[]>();
    for (const ft of result) {
      const arr = grouped.get(ft.path) ?? [];
      arr.push(ft);
      grouped.set(ft.path, arr);
    }
    setPathTags((prev) => {
      const next = new Map(prev);
      // Update entries for the queried paths
      for (const p of paths) {
        const tags = grouped.get(p);
        if (tags) {
          next.set(p, tags);
        } else {
          next.delete(p);
        }
      }
      return next;
    });
  }, []);

  const loadTaggedPaths = useCallback(async (tagId?: number) => {
    const result = await invokeListTaggedPaths(tagId);
    setTaggedPaths(result);
  }, []);

  return {
    tags,
    pathTags,
    taggedPaths,
    loadTags,
    createTag,
    renameTag,
    deleteTag,
    toggleTag,
    loadTagsForPaths,
    loadTaggedPaths,
  };
}
