# Changelog

## [0.6.0] - 2025-06-26

### 📋 Release Summary

This release introduces enhanced session management with smarter context handling and adaptive truncation, improved configuration options including .env key loading, and expanded command capabilities such as line extraction and direct LLM calls (3a99f58, 2fe2d18, 5411078, dc991f2). User experience is improved with clearer cost displays, progress indicators, and robust retry mechanisms (8c52805, fa543c88, f49aef6). Several bug fixes enhance stability, error reporting, and compatibility across sessions, shell commands, and file handling (9ae1a83, af84d01, cf3b0c0).


### ✨ New Features & Enhancements

- **config**: add support for loading keys from .env file `3a99f585`
- **session**: add invisible auto continuation after session reset `2fe2d18d`
- **mcp/fs**: add extract_lines command for line extraction `54110780`
- **session**: add current context tokens to /context output `dc991f26`
- **session**: add adaptive threshold for context truncation `8d2b3414`
- **session**: add smart truncation with internal summarization on to... `8c62e178`
- **session**: add structured summary and file context parsing for co... `c3c593d5`
- **session**: add smart truncation with internal summarization on to... `7d20c893`
- **config**: replace auto-truncation with max session tokens threshold `2a458790`
- **anthropic**: add max-retries rate limiter and enhance retry logging `8c52805e`
- **cli**: add --max-retries flag to session and run commands `a3f2c2e3`
- **ast_grep**: limit glob expansion and improve path handling `9be4366c`
- **ast-grep**: improve output truncation with grouped lines and reus... `e268affa`
- **mcp-shell**: add max_tokens parameter to limit output size `7966360f`
- **chat**: improve cost rendering with intermediate breakdowns and s... `1aef0141`
- **session**: replace custom spinner with indicatif progress bar `f49aef62`
- **chat**: improve cost display with concise static line `fa543c88`
- **mcp**: add call_llm function for direct LLM invocation `4a227a1b`
- **ast_grep**: add glob pattern support for file paths `2c84d591`

### 🔧 Improvements & Optimizations

- **fs**: extract shared file content formatting logic `6e600ef6`
- **mcp**: unify view_file_spec output format for consistency `9788ad3e`
- **session**: unify argument parsing for run commands `c392f325`
- **mcp**: extract MCP initialization into reusable function `acc88534`
- **providers**: unify chat_completion params into struct for cla... `51e882cd`
- **session**: simplify context truncation using summarize method `e3b241cc`
- **fs**: enhance list_files truncation message with stats `b1f6f2ca`

### 🐛 Bug Fixes & Stability

- **session**: correct continuation message processing logic `9ae1a83b`
- **session**: propagate loglevel change to runtime logging macros `3e0cade7`
- **mcp-shell**: follow MCP protocol for shell command responses `af84d019`
- **system-prompts**: ensure all system prompts process templated vari... `90d2d3f0`
- **session**: enforce spending threshold on /run commands `1b797e2f`
- **mcp**: unify response formatting for internal and remote calls `8a830cf3`
- **providers**: log 429 response headers for debug tracing `9656245c`
- **session**: improve spinner color consistency in loading animation `295360fd`
- **agent**: return MCP-compliant errors for invalid inputs and failures `cf3b0c0a`
- **mcp**: return structured errors for invalid ast-grep patterns `ea32caef`
- **session**: preserve tool message order during context truncation `3b46ebaa`
- **session**: lower auto-cache threshold log to debug level `1bcbfea8`
- **commands**: allow optional max_tokens with default fallback `fd238606`
- **session**: preserve and log last assistant summary message after t... `08c3df70`
- **batch_edit**: accept operations as JSON string fallback `463ea146`
- **test**: handle Windows paths in ripgrep output parsing for tests `36f66450`

### 📚 Documentation & Examples

- **shell**: clarify shell command execution and background usage `8cd6f499`
- **session**: improve session continuation prompt clarity and detail `74c628dc`
- **session**: add detailed docs for smart session continuation system `488d4533`
- **fs**: clarify new_str uses raw file content without escaping `46b5aa69`
- **fs**: clarify new_str to avoid escaping and double escaping `e5cc8cb9`
- **fs**: clarify usage guidelines for text editor functions `a281fc64`

### 🔄 Other Changes

- **mcp**: fix shell command tests for MCP response protocol `7a95fd9e`
- **release**: add GitHub Action job to publish crate to crates.io `434487ce`
- **fs**: fix truncation count assertions in fs_tests.rs `13c5b732`
- **fs**: fix file listing test to handle Windows paths `92153860`
- **fs**: fix content search tests for Windows paths" `3d74eccb`
- **fs**: fix content search tests for Windows paths `7a3e8394`

### 📊 Release Summary

**Total commits**: 54 across 5 categories

✨ **19** new features - *Enhanced functionality*
🔧 **7** improvements - *Better performance & code quality*
🐛 **16** bug fixes - *Improved stability*
📚 **6** documentation updates - *Better developer experience*
🔄 **6** other changes - *Maintenance & tooling*

## [0.5.0] - 2025-06-22

### 📋 Release Summary

This release adds support for the DeepSeek AI provider, introduces new session output modes, and enhances code search with the AST-based ast_grep tool. Improvements include more flexible configuration options, detailed cost reporting, and better handling of layered processing contexts. Several bug fixes address file system operations, session stability, tool execution, and server health monitoring to ensure a smoother user experience.


### ✨ Features

- **providers**: add DeepSeek AI provider support (aeb7e707)
- **mcp**: add max_lines and smart truncation to ast_grep output (ab0d498a)
- **session/chat**: improve tool header rendering with context suffix (8157b501)
- **dev**: add ast_grep tool for AST-based code search and refactor (8e4ae7cc)
- **agents**: unify agent configuration with full layer support and a... (056e13c0)
- **session**: add 'last' and 'restart' output modes for layers (5c8a0cf9)
- **cli, config**: add max_tokens option to CLI and config files (49723adf)
- **fs**: add include_hidden option to list_Files function (77f50288)
- **config**: add context_gatherer agent and update workflow instruct... (89553837)
- **session**: display detailed cost breakdown at info log level (9fe01fcc)
- **session**: add sg command version support in helper functions (b4883653)

### 🐛 Bug Fixes

- **ci**: install ripgrep and ast-grep in test workflows (40760793)
- **fs**: correct file listing and content search with ripgrep (def2b93d)
- **fs**: correct ripgrep args and limit list_files output (5cdd1e5e)
- **ast_grep**: pass arguments directly to avoid shell escaping issues (8822e4eb)
- **session**: show tool header for layered processing contexts (f1c896ac)
- **session**: preserve animation during layered response processing (cf00d2b4)
- **agent**: include detailed cost data in agent command results (d62a79ac)
- **session**: resolve rendering issues with layered pipeline output (3773e9ab)
- **session/chat**: correct tool parameter and cost display for agents (956e38d1)
- **fs**: correct line_replace method to replace lines precisely (ae31815e)
- **config**: use root-level max_tokens for all roles (edd6fad8)
- **providers**: remove explicit max_tokens to use default limits (397ee091)
- **layers**: improve Ctrl+C cancellation handling in layers and session (f3031757)
- **layers**: prevent recursive tool calls by checking finish_reason (f773932a)
- **session**: correct layered response user message handling (69b66b4c)
- **config**: remove text_editor tool and developer server ref from MC... (b9c8a8bc)
- **config**: enforce strict layer existence checks for roles (8b3987ff)
- **mcp**: exclude remote HTTP servers on tools/list failure (baa2a82b)
- **animation**: show animation correctly in run command mode (d5dcd16f)
- **session**: correct cache marker placement in parallel tool processing (c11e7659)
- **mcp**: use JSON-RPC POST for HTTP health checks (106e8684)
- **health_monitor**: improve server health checks by type (d7338105)
- **health_monitor**: avoid restarting remote servers as local ones (95f0a5ef)
- **mcp**: improve large output warning with server info and async han... (d95db1e1)
- **session**: enforce session existence on explicit resume (e1870fef)
- **session**: prevent panic by counting chars for content truncation (cf131ee0)
- **mcp**: apply large response threshold consistently in tool calls (b22b22c3)
- **session**: remove assistant messages if any tool results missing (2c3c5d7c)
- **config**: improve developer system prompt clarity and focus (148562b9)
- **tool-execution**: enable immediate cancellation on parallel tool runs (7a8609d5)
- **session/chat**: correct tool header rendering in parallel execution (748f5d63)

### 🔧 Other Changes

- **docker**: add ast-grep and octocode tools to runtime image (f71a3e1a)
- **ast_grep**: group output by file to reduce token usage (38917779)
- **instructions**: clarify development restrictions and code quality... (3f5eac5f)
- add guidelines for efficient Rust development builds (db80bcde)
- **agent**: simplify output merge logic for append mode (4bf2e000)
- clarify code search tools and update usage guidelines (066cbdfa)
- **fs**: add extensive async tests for str_replace function (1d7f81a9)
- **config**: remove hardcoded octocode references from MCP servers (8646c1b7)
- **config**: remove invalid fallback test for unknown role (b97085af)
- **providers**: remove explicit max_tokens to use default limits" (df71e134)
- **logging**: replace eprintln with octomind logging macros (04709e81)
- **layers**: improve layered output formatting and readability (507c4c2f)
- **config**: rename query_processor and context_generator layers... (cc273a9a)
- **config**: enforce explicit system prompt and strict role config (3a4c9563)
- **config**: improve default developer role config and usage guidance (5fd74c50)
- **INSTRUCTIONS**: tune md file with proper guidanance (cc1852b6)
- **tool_map**: derive Default and simplify function pointers (a6d98eb2)
- **mcp**: use static tool map for tool-to-server routing (1168b388)
- **config**: simplify and clarify developer prompt instructions (1ada9b9f)
- **config**: improve mcp.servers config parsing and templates (c05d1fff)
- **config**: revise default developer system prompt for clarity and ... (3b340567)
- **mcp**: split web module into api_client and formatters files (8f1b9fc3)
- **session**: improve MCP tool call rendering for single and mul... (0bdf99f9)

### 📊 Commit Summary

**Total commits**: 65
- ✨ 11 new features
- 🐛 31 bug fixes
- 🔧 23 other changes

## [0.4.0] - 2025-06-17

### 📋 Release Summary

This release introduces new web search tools, including image, video, and news search, along with Brave Search integration and a built-in web server for enhanced functionality. User experience is improved with support for input from stdin, streamlined single-task session handling, and clearer messaging during layered processing. Several bug fixes enhance session stability, output handling, and recursive tool usage.


### ✨ Features

- **config**: add allowed tools filtering from config patterns (0a2add63)
- **web**: add image, video, and news search tools with docs (a98a9abc)
- **config**: add builtin web server with web tools support (662a4dc6)
- **websearch**: add Brave Search integration and web MCP server (a6541e4d)
- **run**: support input from stdin if no parameter provided (46d36043)
- **session**: add user message for automatic layered processing (b5760314)
- **run**: add run command for single-task session handling (0fc4894e)

### 🐛 Bug Fixes

- **session**: disable animation in non-interactive run sessions (864d4206)
- **session**: handle multiple outputs correctly in layered processing (dd5eff99)
- **session**: prevent duplicate user message addition in layered resp... (50b1fc6f)
- **session/layers**: enable recursive tool calls in layers using unif... (256498bf)

### 🔧 Other Changes

- **instructions**: add detailed guidance on where to look first (92fa9661)
- **web-mcp**: move html2md functionality into read_html tool (94b798a7)

### 📊 Commit Summary

**Total commits**: 13
- ✨ 7 new features
- 🐛 4 bug fixes
- 🔧 2 other changes

## [0.3.0] - 2025-06-16

### 📋 Release Summary

This release introduces customizable chat sessions with support for custom instruction files, role-based welcome messages, and enhanced command output handling. Several improvements streamline session context management and configuration options. Multiple bug fixes enhance stability by addressing error handling, session state preservation, and server process isolation.


### ✨ Features

- **session**: add support for custom instructions file in chat sessions (f90c6e61)
- **config**: add role-based welcome messages and %{ROLE} variable (3af97d99)
- **session**: add output_mode handling for command results (8cb4fc57)
- **session**: add filtering to display session context command (2e17ac7e)

### 🐛 Bug Fixes

- **mcp**: return compliant error on user decline of large output (e304ac02)
- **mcp**: isolate server processes to ignore Ctrl+C termination (3da2fce7)
- **session**: remove broken assistant message on empty tool results (a07250ee)
- **session**: remove user message on API call failure to prevent poll... (dab04601)
- **session**: preserve conversation state after tool execution interr... (2b18c71a)

### 🔧 Other Changes

- **docker**: add .dockerignore to exclude unnecessary files (9958f82e)
- **cargo**: remove unused dependencies from Cargo.lock (0001157b)
- **deps**: upgrade multiple dependencies to latest versions (2fa4bfe1)
- **cli**: use dynamic version from Cargo.toml in CLI (3b6c902f)
- **config**: add custom instructions file feature documentation (445e3421)
- **instructions**: add detailed AI project guide and config principles (53d7b004)
- **session**: move context reduction logging after message update (1084f94d)
- **mcp**: replace server_type with type and remove mode field (c6305838)
- **config**: remove octocode availability check and builtin flags (f7a7aeee)
- **commands**: move reduce command from layers to commands defin... (640b4831)
- **layers**: simplify layers and remove unused configs (a40ecca4)
- **changelog**: reformat changelog entries for consistency (d777f1d5)

### 📊 Commit Summary

**Total commits**: 21
- ✨ 4 new features
- 🐛 5 bug fixes
- 🔧 12 other changes

All notable changes to this project will be documented in this file.

## [0.2.0] - 2025-06-14

### 📋 Release Summary

This release enhances session management with new commands like /reduce, /context, dump, and validate for improved user control and feedback, including detailed responses for unknown commands. Tool support is expanded to Amazon and Cloudflare providers, while session stability is improved through better handling of cancellations and tool call preservation. Additional refinements include configurable AI agents for task routing, enhanced prompts, and updated documentation for clearer guidance.


### ✨ Features

- **session**: add detailed feedback for unknown commands (44994ead)
- **session/display**: add token count and percentage per message (1059a5ae)
- **session**: add /reduce command to compress session history (b5aa8047)
- **config**: enhance query_processor and context_generator prompts (fe3bbf41)
- **session**: add tool support to Amazon and Cloudflare providers (b6488700)
- **session**: add /context command to display session context (809d3929)
- **fs**: enhance line replacement feedback with detailed snippet and... (6b0cf942)
- **agent**: add configurable AI agents for task routing (42c7cb45)
- **config**: add parsing support for custom roles in config (4f2f1b6e)
- **session**: add dump and validate commands for MCP tools (47c61946)

### 🐛 Bug Fixes

- **session**: update debug toggle command in display message (33019763)
- **mcp**: preserve server process on cancellation (e7b7923c)
- **session**: clean up tool_calls on Ctrl-C cancellation (1462e056)
- **session/list**: add markdown rendering with plain text fallback (8276cba9)
- **session**: ensure tool_calls match results after tool execution (9f4f0e22)
- **session**: clean up incomplete tool_calls on interrupt (a7286a9e)
- **session**: preserve valid tool requests on Ctrl+C interruption (79b6c475)
- **session**: reset full session context on Ctrl+C cancellation (98fbae08)
- **commands**: disable MCP tools for ask and shell commands (8a1e6f7b)
- **session**: sort tool functions to ensure consistent order (d55915e4)
- **session**: remove /debug command and make /loglevel runtime-only (0ef1594d)
- **session**: safely truncate strings by counting chars instead of bytes (3bcc67d5)
- **config**: enforce explicit temperature in role configs (fb335b25)
- **session**: ensure immediate cancellation on Ctrl+C during follow-up (d678183c)
- **session**: preserve complete tool sequences during truncation (a411d4e2)

### 🔧 Other Changes

- **fs**: reduce prompt tokens in MCP function definitions (29b0f28b)
- **providers**: move providers out of session module (1a34c663)
- **session**: split chat commands into separate files (e8ffcd80)
- **fs**: enhance text editor command usage guidance and examples (ab184809)
- **config**: document layered architecture with named layers (b9fc0dbd)
- add asciinema demo to README (a4cd5fb5)
- **config**: update config file location to system-wide directory (605b9c89)
- **fs**: clarify text_editing tool definitions and usage warnings (01d57dbd)
- **config**: rename mode to role across codebase (c96dc3da)
- **session**: unify tool-to-server lookup for /mcp command (b3678a52)
- **config**: rename get_mode_config to get_role_config consistently (dcbb882c)
- add Cargo.lock to repository tracking (243dc8ab)
- **config**: clarify agent configs and update examples (517e58ec)
- **chat**: unify tool execution for sessions and layers (7ed9af58)
- **mcp**: add MCP result helpers and improve undo output (50647017)
- **mcp**: add tool-to-server map for routing tool calls (9dcb710a)
- **config**: unify role configs using roles array format (208b7251)
- **deps**: upgrade multiple dependencies and add new crates (ceeece54)

### 📝 All Commits

- 33019763 fix(session): update debug toggle command in display message *by Don Hardman*
- e7b7923c fix(mcp): preserve server process on cancellation *by Don Hardman*
- 1462e056 fix(session): clean up tool_calls on Ctrl-C cancellation *by Don Hardman*
- 8276cba9 fix(session/list): add markdown rendering with plain text fallback *by Don Hardman*
- 9f4f0e22 fix(session): ensure tool_calls match results after tool execution *by Don Hardman*
- 44994ead feat(session): add detailed feedback for unknown commands *by Don Hardman*
- 29b0f28b refactor(fs): reduce prompt tokens in MCP function definitions *by Don Hardman*
- 1059a5ae feat(session/display): add token count and percentage per message *by Don Hardman*
- a7286a9e fix(session): clean up incomplete tool_calls on interrupt *by Don Hardman*
- 1a34c663 refactor(providers): move providers out of session module *by Don Hardman*
- e8ffcd80 refactor(session): split chat commands into separate files *by Don Hardman*
- b5aa8047 feat(session): add /reduce command to compress session history *by Don Hardman*
- 79b6c475 fix(session): preserve valid tool requests on Ctrl+C interruption *by Don Hardman*
- fe3bbf41 feat(config): enhance query_processor and context_generator prompts *by Don Hardman*
- 98fbae08 fix(session): reset full session context on Ctrl+C cancellation *by Don Hardman*
- ab184809 docs(fs): enhance text editor command usage guidance and examples *by Don Hardman*
- 8a1e6f7b fix(commands): disable MCP tools for ask and shell commands *by Don Hardman*
- b9fc0dbd docs(config): document layered architecture with named layers *by Don Hardman*
- a4cd5fb5 docs: add asciinema demo to README *by Don Hardman*
- 605b9c89 docs(config): update config file location to system-wide directory *by Don Hardman*
- b6488700 feat(session): add tool support to Amazon and Cloudflare providers *by Don Hardman*
- d55915e4 fix(session): sort tool functions to ensure consistent order *by Don Hardman*
- 0ef1594d fix(session): remove /debug command and make /loglevel runtime-only *by Don Hardman*
- 809d3929 feat(session): add /context command to display session context *by Don Hardman*
- 01d57dbd docs(fs): clarify text_editing tool definitions and usage warnings *by Don Hardman*
- 6b0cf942 feat(fs): enhance line replacement feedback with detailed snippet and... *by Don Hardman*
- c96dc3da refactor(config): rename mode to role across codebase *by Don Hardman*
- b3678a52 refactor(session): unify tool-to-server lookup for /mcp command *by Don Hardman*
- dcbb882c refactor(config): rename get_mode_config to get_role_config consistently *by Don Hardman*
- 243dc8ab chore: add Cargo.lock to repository tracking *by Don Hardman*
- 517e58ec docs(config): clarify agent configs and update examples *by Don Hardman*
- 3bcc67d5 fix(session): safely truncate strings by counting chars instead of bytes *by Don Hardman*
- 7ed9af58 refactor(chat): unify tool execution for sessions and layers *by Don Hardman*
- 42c7cb45 feat(agent): add configurable AI agents for task routing *by Don Hardman*
- fb335b25 fix(config): enforce explicit temperature in role configs *by Don Hardman*
- d678183c fix(session): ensure immediate cancellation on Ctrl+C during follow-up *by Don Hardman*
- 50647017 refactor(mcp): add MCP result helpers and improve undo output *by Don Hardman*
- 9dcb710a refactor(mcp): add tool-to-server map for routing tool calls *by Don Hardman*
- 208b7251 refactor(config): unify role configs using roles array format *by Don Hardman*
- 4f2f1b6e feat(config): add parsing support for custom roles in config *by Don Hardman*
- 47c61946 feat(session): add dump and validate commands for MCP tools *by Don Hardman*
- ceeece54 chore(deps): upgrade multiple dependencies and add new crates *by Don Hardman*
- a411d4e2 fix(session): preserve complete tool sequences during truncation *by Don Hardman*

## [0.1.0] - 2025-06-10

## Your AI Development Companion is Here!

We're excited to announce the first official release of **Octomind** - an AI-powered development assistant that transforms how you interact with your codebase through natural conversations.

## 🎯 What Makes This Release Special

**Session-First Development** - No more complex CLI commands or setup. Just start a conversation with AI and get things done. Whether you're debugging, refactoring, or exploring new code, Octomind understands your project context and helps you work smarter.

**Multi-Provider AI Support** - Choose from OpenRouter, OpenAI, Anthropic, Google, Amazon, or Cloudflare. Switch between models on the fly and find the perfect AI assistant for your specific task.

**Built-in Development Tools** - File operations, code analysis, shell commands, and more - all accessible through natural conversation. No need to leave your AI session to get work done.

## ✨ Key Features in v0.1.0

- 🤖 **Interactive AI Sessions** with intelligent context management
- 🛠️ **Integrated Development Tools** via MCP protocol
- 🌐 **6 AI Provider Integrations** with unified interface
- 🖼️ **Multimodal Vision Support** - analyze images, screenshots, and diagrams
- 💰 **Real-time Cost Tracking** with detailed usage reports
- 🔧 **Role-Based Configuration** - Developer and Assistant modes
- 📊 **Smart Caching System** for cost optimization
