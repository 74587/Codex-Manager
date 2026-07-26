# PR Draft: 修复桌面全屏账号表操作列偏移

## 标题

修复桌面全屏账号表操作列偏移

## 变更摘要

- 移除右侧工作区的全局 90% 缩放层，恢复 Header 与 main 直接位于主列下的普通布局。
- 将账号表 sticky 操作列背景改为实色，避免横向滚动内容从操作列下方透出。
- 为账号表设置稳定最小宽度，并更新源码回归测试，锁住主内容普通布局。

## 改动范围

- [x] Frontend
- [x] Desktop / Tauri
- [ ] Service
- [ ] Gateway / Protocol Adapter
- [x] Docs / Governance
- [ ] Workflow / Release

## 主要文件

- `apps/src/components/layout/app-frame.tsx`
- `apps/src/app/accounts/accounts-page-view.tsx`
- `apps/src/app/globals.css`
- `apps/tests/app-frame-scale.test.mjs`
- `docs/superpowers/plans/2026-07-27-desktop-sticky-action-column-pr-draft.md`

## 验证

- [x] `pnpm -C apps run test`
- [x] `pnpm -C apps run build`
- [ ] `pnpm -C apps run test:ui`
- [ ] `cargo test --workspace`
- [x] 其他本地验证已说明

已执行的实际验证：

```text
pnpm -C apps run test:runtime
pnpm -C apps run lint
pnpm -C apps run build:desktop
git diff --check
```

验证结果：

```text
test:runtime: 189 passed
lint: passed
build:desktop: passed
git diff --check: passed
```

未执行的验证与原因：

```text
cargo test --workspace 未执行。本次改动只涉及 apps 前端布局、CSS 和前端测试，没有修改 Rust service、Tauri 命令或网关协议代码。
pnpm -C apps run test:ui 未执行。当前提交只保留源码回归测试。
```

## 风险与影响面

- 宽屏桌面主内容区恢复为 100% 普通布局，之前依赖整块 90% 缩放的视觉密度会回到局部 CSS 控制。
- sticky 操作列改为实色背景后，现代外观下该列的玻璃透明感降低，换来内容遮挡稳定性。
- 账号表设置最小宽度后，小宽度窗口通过表格容器横向滚动访问完整列。

## 备注

- 问题引入点是 `3fa3572f feat(ui): restore shell features and dashboard wiring` 中新增的 `app-main-scale`。
- 当前修复保留作者想要的宽屏可用性目标，把实现方式从整块缩放收敛到局部表格与操作列布局。
- 提交前请确认未包含敏感 token、cookie、API key。
