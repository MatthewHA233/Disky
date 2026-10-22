import type { ArchiveCategory } from "../../types";

/** 三个门类的显示名与颜色（进度条分段、页签标记用） */
export const CATEGORY_META: Record<ArchiveCategory, { name: string; color: string; hint: string }> = {
  software: { name: "软件", color: "#0F6CBD", hint: "装在电脑上的程序，按使用目的分箱" },
  personal: { name: "个人文件", color: "#C08A00", hint: "不属于任何软件的文件夹，按内容分箱" },
  devenv: { name: "插件与开发环境", color: "#8764B8", hint: "运行时、命令行工具、扩展这类装在别的东西里面的" },
};
