# Pals 翻新设计文档 — AI 剧组模式

> 日期: 2026-07-01
> 状态: 设计稿

## 1. 概述

将当前的 Pals（角色/Character）功能从一个"单角色选择器"升级为**多角色协作对话系统**，用户通过 `@` 点名或在后台导演 LLM 的调度下，让多个不同角色（每个有自己的模型、system prompt、参数）在同一段对话中交替发言。

## 2. Pal 定义

Pal 是一个**可复用的角色卡（character card）**，包含三个要素：

| 要素 | 字段 | 说明 |
|---|---|---|
| **身份** | name, alias, avatar, role_bio | 让用户和导演 LLM 知道"这个 Pal 是什么角色" |
| **行为** | system_prompt, description | 定义它怎么说话、怎么思考、擅长什么 |
| **引擎** | model_id, parameters | 绑定到具体模型 + 推理参数 |

Pal 的边界：

- ✅ Pal = 身份 + 行为配置 + 模型绑定
- ❌ Pal 没有自己的对话历史/长期记忆（那是 Conversation 的事）
- ❌ Pal 不是独立运行的代理进程（被 @ 时激活，用完即走，不持有状态）
- ❌ Pal 没有独立的工具/MCP 绑定（未来可能扩展，当前不做）

### 核心比喻

一条对话是一个**舞台**，Pals 是**演员**，系统内置的**导演机制（orchestration layer）**负责调度：

- `@某Pal 消息` → 用户点名，直接路由到该 Pal
- 无 @ → 默认回复角色（default responder）回复
- 导演机制在每条回复后，审查最近对话，判断是否让其他已登场的 Pal 插入发言
- 仅已被用户 **显式 @ 过** 的 Pal 才能被导演机制调度（防止乱入）

---

## 3. 交互规则

### 3.1 @ 点名路径

```
用户输入: "@写作助手 帮我想个标题"
   ↓
解析 @写作助手 → 查找对应 Pal
   ↓
路由到写作助手的: model_id + system_prompt + parameters
   ↓
🤖 写作助手回复 (UI 标记 "用户点名")
```

- `@` + 角色名触发 **autocomplete 下拉**
- 消息发送时，前端解析 `@角色名` 提取 pal_id
- 后端请求使用该 Pal 的完整配置（model / provider / prompt / params）
- 回复气泡上显示该 Pal 的头像和名字

### 3.2 无 @ 路径

```
用户输入: "这段代码跑起来好慢啊"
   ↓
无 @ 匹配 → 走默认回复角色
   ↓
🤖 默认回复角色 回复
```

- 每个对话有一个**默认回复角色（default responder）**，用户可在新建对话时指定
- 系统内置一个**导演机制（director / orchestration layer）**，负责在回复后判断是否调度其他 Pal
- V1 中，默认回复角色是用户配置项；导演机制不是 Pal，也不需要单独配置 id
- 若用户未指定默认回复角色，则回退到全局默认回复角色
- 如果缺少默认回复角色，提示用户完成选择

### 3.3 导演调度路径

任何 Pal 回复完毕后，导演机制审查最近对话，决定是否让其他已被用户显式 @ 过的 Pal 插入发言。

```
（任一 Pal）回复完毕
   ↓
导演机制审查最近 5-10 条消息 + 已被用户显式 @ 过的 Pal 列表
   ↓
判断逻辑:
  - "这个话题适合 @代码审查 介入"
  → 自动调用代码审查的 model + prompt 回复
  → UI 标注 "导演调度"

  或:
  - "不需要其他角色加入"
  → 安静等待下一条用户消息
```

**导演机制的 prompt 设计方向（由系统内置 orchestration prompt 提供，不属于任何 Pal）：**

```
你是一个多角色对话的导演机制。你的职责：
1. 观察对话走向，判断是否有其他角色更有资格回应
2. 在合适的时候调度其他角色插入发言
3. 如果没有合适角色，就保持安静

你可以调度的角色列表（仅限用户已 @ 过的）：
{pals_list_each_with_name_and_description}

每轮你回复完后，如果觉得某个角色能提供不同视角或专业
意见，请输出调度指令。否则保持安静。
```

### 3.4 多 @ 与防死循环规则

| 规则 | 说明 |
|---|---|
| 多 @ 支持 | 单条用户消息允许 `@pal1 @pal2 @pal3` |
| 多 @ 回复顺序 | 按用户在输入中出现的先后顺序依次生成回复 |
| 多 @ 回复关系 | 后一个 Pal 可以看到前一个 Pal 在同条用户消息链路中的补充内容，并继续增补信息 |
| 导演调度上限 | 每条用户消息后，导演最多调度 **1 个** Pal |
| Pal 回复上限 | 每个 Pal 在单条用户消息后最多回复 **1 次** |
| 导演上下文窗口 | 只审查最近 **5-10 条**消息做判断 |
| 禁止重复调度 | 导演机制不能在同一轮里重复调度同一个 Pal |
| 导演调度范围 | 只能调度已被用户**显式 @ 过**的 Pal；仅被导演调度过不算解锁 |
| 用户打断 | 用户发新消息 → 立即终止当前正在进行的 Pal 回复 |

---

## 4. UI 设计

### 4.1 聊天视图改动（Chat.vue）

```
┌──────────────────────────────────────────────────┐
│ 🔧 后端工程师 · 📋 项目经理    🤖 导演模式    [+] │
│   └── 已登场的 Pal 头像列表 ──    └── 设置默认 Pal │
├──────────────────────────────────────────────────┤
│                                                  │
│ 👤 我：@后端工程师 设计一下用户认证                  │
│ ┌────────────────────────────────────────┐       │
│ │ 🔧 后端工程师    📍 用户点名           │       │
│ │ 用 JWT + refresh token，结构上...       │       │
│ └────────────────────────────────────────┘       │
│                                                  │
│ ┌────────────────────────────────────────┐       │
│ │ 📋 项目经理    🎬 导演调度             │       │
│ │ 这个方案 MVP 太重了...                  │       │
│ └────────────────────────────────────────┘       │
│                                                  │
├──────────────────────────────────────────────────┤
│ 📝 @后端工程师 那 middleware 怎么处理...  📎 📷 🚀│
└──────────────────────────────────────────────────┘
```

**组件拆解：**

| 组件 | 说明 |
|---|---|
| `ChatPalBar` | 对话顶部的 Pal 成员栏，显示已登场的 Pal 头像列表，hover 显示角色名和模型 |
| `MessageBubble` | 现有组件扩展：添加 `pal` 信息（头像、名称）、`source` 标记（用户点名 / 导演调度） |
| `PalAutocomplete` | @ 输入时的下拉提示，基于所有 Pal（不仅已登场），输入即搜索 |
| `DirectorIndicator` | 导演机制触发调度时仅显示轻量状态标记，不展示调度 reason |

### 4.2 PalsView 改动（Pal 管理页）

保持现有 CRUD 能力，新增：

- **默认角色设置**：每个 Pal 可设为"默认回复角色"
- **导演可见信息**：Pal 编辑表单新增 "角色简介"(role_bio) 字段，用于导演机制判断是否调度该 Pal
- **登场记录**：可选，显示每个 Pal 最近在哪些对话中被 @ 过

### 4.3 新建对话流程

```
点击 "新建对话"
   ↓
弹出选人面板
  ┌────────────────────────────┐
  │ 选择默认回复角色           │
  │ ┌────────────────────────┐ │
  │ │ 🔧 后端工程师     ★   │ │
  │ │ 📋 项目经理            │ │
  │ │ 🎨 设计师              │ │
  │ │ ...                    │ │
  │ └────────────────────────┘ │
  │ 📝 对话名称: _____________ │
  │ [  开始对话  ]              │
  └────────────────────────────┘
```

- 至少选择 1 个默认回复角色
- 若已有全局默认配置，自动预选
- 导演机制始终存在，不需要用户选择
- 进入对话后，随时可 @ 新 Pal 登场

### 4.4 角色首次登场的仪式感

当用户第一次 `@某Pal` 时：

1. 发送的消息正常发送
2. 在目标 Pal 回复之前，插入一条**系统消息**:
   > 🎭 **写作助手** 加入了对话
3. 然后该 Pal 的回复紧随其后

---

## 5. 技术与数据流

### 5.1 消息发送流程

```
用户发送消息
   ↓
① 前端解析 @ 引用
   ↓
② ┌─ 有 @? ──→ 调用 conversationSendMessage 附带 target_pal_ids
   │               后端按顺序使用各 Pal 的 model + prompt 依次生成回复
   │
   └─ 无 @? ──→ 调用 conversationSendMessage 不附带 target_pal_ids
                  后端使用默认回复角色配置
   ↓
③ AI 回复
   ↓
④ 回复完成后，触发导演检查
   ↓
⑤ 导演检查:
   ┌─ 决定调度 → 导演输出调度指令
   │           → 后端调用目标 Pal 的配置生成回复
   │           → 回复发送到前端
   │           → 回到步骤④（但跳过该 Pal，防止递归）
   │
   └─ 不调度  → 等待用户下一条消息
```

### 5.2 数据模型扩展

```typescript
// Character 扩展字段
interface Character {
  // ... 现有字段
  role_bio: string;        // 新增: 角色简介，导演机制用
}

// Conversation 扩展字段
interface Conversation {
  // ... 现有字段
  default_pal_id?: string;   // 新增: 默认回复角色

}

// Message 扩展
interface Message {
  // ... 现有字段
  pal_id?: string;         // 新增: 回复这条消息的 Pal ID
  pal_name?: string;       // 新增: 回显用，方便前端展示
  source: 'user_prompted' | 'directed';  // 新增: 回复来源
}

// ConversationSendRequest 扩展
interface ConversationSendRequest {
  // ... 现有字段
  target_pal_ids?: string[];  // 新增: @ 点名时指定，按顺序路由
}
```

### 5.3 导演检查的触发位置

**选项 A：后端触发（推荐）**

回复流完成后，Rust 后端自动调用导演检查：

```
AI 回复完成
   ↓
任何 Pal 回复完成:
  → 触发导演检查
   ↓
导演检查 → 后端组装 orchestration prompt（含上下文 + 已被用户显式 @ 过的 Pal 列表）
   ↓
调用系统内置导演机制判断
   ↓
如果输出调度指令 → 调用目标 Pal 回复
如果输出 none → 等待
```

**选项 B：前端触发**

回复流完成后，前端判断是否让用户再点一次发送...但这个太繁琐，不推荐。

**采用选项 A。**

### 5.4 @ 解析实现

纯前端解析，不依赖后端：

```typescript
// 在 Chat.vue 中
const AT_MENTION_REGEX = /@(\w+)/g;

function parseAtMentions(input: string): {
  text: string;
  palIds: string[];
} {
  const palIds: string[] = [];
  const text = input.replace(AT_MENTION_REGEX, (match, name) => {
    const pal = characterStore.characters.find(
      c => c.name === name || c.alias === name
    );
    if (pal) {
      palIds.push(pal.id);
      return match; // 保留 @文本 展示
    }
    return match;
  });
  return { text, palIds };
}
```

---

## 6. 需要修改的文件

| 文件 | 改动 |
|---|---|
| `src/libs/types.ts` | Message 扩展 pal_id/pal_name/source；Character 加 role_bio |
| `src/stores/chat.ts` | 支持 target_pal_id；发送后触发导演检查 |
| `src/stores/character.ts` | 新增角色简介字段管理 |
| `src/components/Chat.vue` | 顶部 PalBar；@ autocomplete；消息气泡显示 Pal 信息 |
| `src/components/ChatPalBar.vue` | 新增：已登场 Pal 头像栏 |
| `src/components/PalAutocomplete.vue` | 新增：@ 输入提示 |
| `src/components/MessageBubble.vue` | 扩展：Pal 头像/名称 + 来源标记 |
| `src/views/PalsView.vue` | 默认 Pal 设置；角色简介字段 |
| `src/components/CharacterForm.vue` | 角色简介字段 |
| `src-tauri/src/commands.rs` | 新增导演检查命令或参数 |
| `src-tauri/src/configs/character.rs` | role_bio 字段 |

---

## 7. 实现优先级

### Phase 1：基础设施 + @ 解析
- types 扩展（pal_id, source, role_bio）
- Rust 后端：角色路由（target_pal_id）、Message 扩展字段
- 前端：@ autocomplete 组件
- 消息气泡扩展：显示 Pal 名字/头像

### Phase 2：导演调度
- 后端导演检查逻辑
- 导演 prompt 模板
- 调度指令的 tool_use / 结构化输出
- 防死循环机制

### Phase 3：UI 交互增强
- ChatPalBar 成员栏
- 角色登场仪式感（系统消息动画）
- 新建对话选人面板
- 默认 Pal 设置

---

## 8. 打开问题

1. **导演机制的模型调用成本**：每次回复后多一次 LLM 调用。需要决定它是否复用默认回复角色所绑定的模型，还是固定使用一个更轻量的内置模型。
2. **导演输出格式**：需要定义标准化的调度指令 JSON 格式，如 `{"action": "invoke", "pal_id": "xxx", "reason": "..."}`，用 tool_use 或结构化输出来保证解析可靠性。
3. **消息归属**：导演调度产生的 Pal 回复，其父消息应该是用户消息还是默认 Pal 的消息？应该是用户消息，这样对话树保持以用户消息为根。
4. **@ 自动补全的性能**：如果 Pal 数量多，下拉搜索需要做防抖 + 本地模糊搜索。
