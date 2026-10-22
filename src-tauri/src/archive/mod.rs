//! 档案馆：跨盘汇总「每个软件占了多少、散在哪、有哪些能清理」，再由用户 / AI 按使用目的装箱。
//!
//! - `digest`：每块盘扫描完成后提炼的目录摘要（扫描树只在内存里保留当前一块盘）
//! - `registry`：系统登记的已安装软件（Windows 卸载信息）
//! - `rules`：内置规则——数据目录别名、开发工具链、常见软件的用途
//! - `inventory`：把摘要和已安装软件合成「软件 → 位置」清单，以及没认出归属的目录树
//! - `icons`：从 exe 提取图标
//! - `growth`：和上次扫描比，软件、文件夹变大还是变小了多少
//! - `exe_info`：读 exe 的厂商、产品名（给 AI 查归属用）
//! - `store`：箱子与装箱结果

pub mod digest;
pub mod exe_info;
pub mod growth;
pub mod icons;
pub mod inventory;
pub mod registry;
pub mod rules;
pub mod store;
