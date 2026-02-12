# Changelog

## [0.17.0] - 2026-02-12

### 📋 Release Summary

This release introduces intelligent conversation compression that automatically manages context size and costs while preserving important information, plus new keyboard shortcuts and file completion for faster navigation. Enhanced visual feedback includes progress animations, context percentage displays, and helpful tips during sessions. Multiple bug fixes improve token tracking, compression reliability, and overall system stability.


### ✨ New Features & Enhancements

- **config**: add OCTOMIND_CONFIG_PATH environment variable support `cc9a799f`
- **compression**: preserve plan context in compressed task summaries `414e34ff`
- **chat**: detect task completion patterns as critical chunks `f7e52081`
- **compression**: add ignore_cost flag for decision API `337b60d9`
- **compression**: add context-aware bounds for session estimation `827bc57e`
- **compression**: adaptive compression with real pricing `c0c666eb`
- **chat**: add Ctrl+E to exit reverse search `99da1fcc`
- **tool**: display animation during tool execution `79bce9eb`
- **info**: show conversation compression statistics `0824582a`
- **compression**: adaptive pressure-based compression `17573f05`
- **chat**: add animation feedback during compression `0fa57c5c`
- **compression**: add decision model for cost savings `eb45f89b`
- **chat**: add automatic conversation compression `79ba40cf`
- **compression**: add adaptive compression with context pressure detection `f1935080`
- **session**: display randomized tips on new sessions `e93ce76b`
- **edit_mode**: add Meta key bindings for word operations `0798b3a7`
- **input**: add Ctrl+a/Ctrl+e/Ctrl+u navigation shortcuts `7c74d375`
- **chat**: add @ file completion `4942ffb7`
- **chat**: add context percentage display to prompt and animations `b7dcd2e0`
- **modal**: add terminal overlay system for help tooltips `20d6d3a4`
- **config**: add multi-file configuration support `aae24a2b`
- **workflow**: improve execution output with progress tracking `7d7e42bf`
- **commands**: add workflow command, remove layers command `009d85e6`
- **ui**: add progress spinner for MCP server initialization `3d3d7b96`
- **mcp**: add OAuth 2.1 + PKCE support for HTTP MCP `c4a7dd35`

### 🔧 Improvements & Optimizations

- **session**: persist runtime state in SessionInfo `35beae70`
- **chat**: preserve file context in compression `aa123c6f`
- **compression**: use struct for decision model config `c6e1a4e0`
- **compression**: skip compression when no token savings `9e599532`
- **token counting**: rename and improve token estimation `81d7c37d`
- **mcp**: improve edit methods documentation `a178e4b3`
- **compression**: remove pressure_trigger and consolidate threshold logic `afdfdf2d`
- **conversation-compression**: merge decision and summary into single API call `5be9e6d2`
- **diff_chunker**: replace summarization with semantic chunking `c9f3aac3`
- **plan**: simplify compression tracking by removing execution guard `70084746`
- **compression**: implement automatic hierarchical compression system `7a4d3669`
- **session**: auto-process summary requests during continuation `83819969`
- **session**: move tip display function to setup module `6538ab8e`
- **chat**: make queue add action invisible to user `b474074a`
- **chat**: simplify key condition and enable bracketed paste `3390bb46`
- use debug logging for cancellation messages `74acfb57`
- **chat**: migrate to reedline for terminal input `03e8ee86`
- **session/chat**: extract runner into focused modules `ff52e90a`
- **session**: improve interrupt feedback handling `d2088a35`
- **session**: improve exit flow and UI feedback `4b322eaa`
- **workflows**: use iterator pattern for workflow access `36b29ff9`
- **workflows**: improve session and output handling `82c9796d`
- **session**: migrate to workflow architecture `99524dff`
- **text_editor**: improve argument names for clarity `d374541e`
- **chat**: add response_id tracking for OpenAI API `fcd4a89f`

### 🐛 Bug Fixes & Stability

- **plan**: enable automatic continuation after compression in plan `9e2d26a9`
- **session**: prevent token drift and message reload after compression `6cf67d43`
- **compression**: prevent cutting tool messages during compression range calculation `37151054`
- **compression**: reset token counters after compression `84c8ba51`
- **layers**: add missing animation for follow-up API calls `512ef18d`
- **chat**: prevent cutting between assistant tool calls and results `135bfde3`
- **compression**: include first compression message in range calculations `c4a86534`
- **chat**: remove duplicate token tracking in response handlers `d221aedb`
- **chat**: clarify token breakdown labels and cache comment `cf183628`
- **compression**: preserve cache markers during compression `4da7044e`
- **moonshot**: preserve thinking field in tool result processing `98ea2c17`
- **providers**: add error handling for thinking field deserialization `766ad26b`
- **moonshot**: resolve usage tracking and thinking block handling `c30a345b`
- **moonshot**: handle thinking field conversion for reasoning models `7f541520`
- **web**: handle stringified JSON array input to prevent crash `c7b6f478`
- **glob**: normalize paths before pattern matching `e3ad81e4`
- **compression**: correct cache calculation for decision model `dfbf7989`
- **compression**: add debug logging and prevent start_index reset `1cb1b335`
- **compression**: be consistent with effeciency tracking `807d5002`
- **compression**: corruption with missing tools in some cases `cbded7d7`
- **compression**: cache-aware smart adaptive compression logic `ce69f067`
- **tokens**: unify tokens calculation `cfe32edf`
- **compression**: discourse-aware semantic chunking `8355f513`
- **animation**: fix issue with working animation showed before tool execution `c285d150`
- **session/chat**: accurately estimate context with system prompt and tools `c5bb42de`
- **compression**: correct token counting for threshold checks `12867e1f`
- **chat**: add sleep to avoid animation race `e02dbfbf`
- **plan**: adjust start index for tool preservation in compression `dcb9cfc6`
- **plan**: skip compression logic when none pending `953ac2fe`
- **message**: correct inclusive range boundary validation `6eaf6641`
- **input**: add missing $ to cost display indicator `693b9984`
- **session/chat**: handle Ctrl+C during reverse search `9bd34f99`
- **session**: display resume command on Ctrl+D with session ID `47dcae03`
- **session**: show context status line only at session start `6ed1a391`
- **context**: compute accurate context token usage `bf46318b`
- **session**: clean up user message on cancellation during API call `0016e3e1`
- **session**: improve operation tracking for session state `13838bfc`
- **session**: preserve conversation state when API call interrupted after tools `b6831be0`
- **providers**: update response_id field to id for octolib compatibility `db7ee3f0`
- **chat**: skip history for whitespace-prefixed inputs `b133f6b2`
- show correct provider name in cost error messages `52012b22`
- **health_monitor, oauth**: adjust health check interval and fix OAuth discovery `eb6c752d`
- **zai**: resolve provider issue `59523442`

### 📚 Documentation & Examples

- **providers**: update provider format and supported list `c487a93a`
- add compression and multi-file config features `1229e7c0`
- **advanced**: expand future turn estimation with velocity-based analysis `92cee9f3`
- **sessions**: add visual feedback and animation documentation `e825dd04`
- add compression and token system documentation `de0564bd`
- significantly condense INSTRUCTIONS.md developer guide `3f37c65b`
- document config, auth, and workflow features `45bb09ba`
- **installation**: detail shell completion setup for all shells `a531d95a`

### 🔄 Other Changes

- **chat**: extend compression range test for multiple conversation turns `29db9436`
- **deps**: bump octolib to 0.8.0 `31d918ec`
- upgrade Rust toolchain to 1.92.0 `a4979870`
- **ci**: bump Rust toolchain to 1.92.0 `08071883`
- **deps**: update dependency versions in Cargo.lock `b9e6d201`
- upgrade octolib to 0.7.0 `d2bb7896`
- **deps**: upgrade clap to 4.5.56 and rand to 0.9 `74319072`
- **deps**: update octolib to latest version `a66d9ea4`
- **deps**: upgrade cargo dependencies `30a7004c`
- **fs**: update tests to use lines parameter instead of view_range `9ee929a9`
- **deps**: switch local octolib path to version 0.5.0 `5d9b1503`
- **oauth**: fix validation tests for public client scenarios `166b83f8`
- **deps**: bump octolib to 0.4.2 `860e1439`

### 📊 Release Summary

**Total commits**: 114 across 5 categories

✨ **25** new features - *Enhanced functionality*
🔧 **25** improvements - *Better performance & code quality*
🐛 **43** bug fixes - *Improved stability*
📚 **8** documentation updates - *Better developer experience*
🔄 **13** other changes - *Maintenance & tooling*

## [0.16.0] - 2026-01-09

### 📋 Release Summary

This release enhances the conversational experience with thinking headers in messages and structured command outputs (487fe367, b20c975c). Support for zai and minimax AI providers has been added via updated dependencies, alongside improved thinking displays (fae757c1, 1c0add41). Several bug fixes boost file search reliability, cross-platform glob handling including Windows paths, chat consistency, and AI tool integration (ea7ffba4, 5e0405ff, c82d941e, 18dfb4b1, 8fb6d48d, a86e558e, 4fa2360c, dcc7a8ed, c058a6b6).


### ✨ New Features & Enhancements

- **session**: add thinking header to messages `487fe367`
- **server,session**: structured command outputs for websocket server `b20c975c`

### 🔧 Improvements & Optimizations

- **thinking_display**: use fixed separator pattern `1c0add41`
- **display**: centralize list rendering `4eea10c2`
- **session**: simplify message handler `9e52cd26`

### 🐛 Bug Fixes & Stability

- **mcp/fs**: replace map_or with is_some_and for content check `ea7ffba4`
- **fs**: skip content search for empty content `5e0405ff`
- **chat**: prevent duplicate thinking block display `c82d941e`
- **utils**: improve glob pattern handling for Windows paths `18dfb4b1`
- **glob**: handle absolute path patterns without base_dir `8fb6d48d`
- **ast_grep**: fail when globs match no files `a86e558e`
- **session/chat**: warn on empty continuation summary `4fa2360c`
- **file-parser**: prefer <context> tags `dcc7a8ed`
- **gemini**: preserve meta in tool calls `c058a6b6`
- **docker**: update Rust version to `ca1c73ce`

### 📚 Documentation & Examples

- **mcp**: update tool docs and server enhancements `7f48c516`

### 🔄 Other Changes

- **deps**: update octolib to v0.4.0 with zai and minimax providers `fae757c1`
- **deps**: update octolib to 0.3.0 `f4dbc3e7`
- **deps**: bump dependency versions `8f53df17`
- **deps**: update deps and WS API `52478897`
- **workflows**: add missing newline at EOF `a3a70ca5`

### 📊 Release Summary

**Total commits**: 21 across 5 categories

✨ **2** new features - *Enhanced functionality*
🔧 **3** improvements - *Better performance & code quality*
🐛 **10** bug fixes - *Improved stability*
📚 **1** documentation update - *Better developer experience*
🔄 **5** other changes - *Maintenance & tooling*

## [0.15.0] - 2025-11-21

### 📋 Release Summary

This release adds the ability to resume recent sessions and include custom constraints in user inputs, enhancing workflow flexibility (fae183ed, be8c2bd2). It also integrates new AI models, Claude Sonnet 4.5 and GPT-5-Codex, expanding the assistant's capabilities (07a67cc2). Several fixes improve session history migration and command handling, while updates streamline dependencies and provider management for a more reliable experience (7c58af9a, f02967dd, 777bb6d3, 1953a4df).


### ✨ New Features & Enhancements

- **session**: add --resume-recent flag to resume latest session `fae183ed`
- **session**: add support for appending custom constraints to user input `be8c2bd2`
- **models**: add support for Claude Sonnet 4.5 and GPT-5-Codex `07a67cc2`

### 🔧 Improvements & Optimizations

- **providers**: unify providers via octolib and remove legacy code `1953a4df`

### 🐛 Bug Fixes & Stability

- **commands**: remove redundant return statements in ask.rs `7c58af9a`
- **session**: handle legacy history file migration correctly `f02967dd`

### 🔄 Other Changes

- **deps**: remove unused dependencies and clean Cargo.lock `777bb6d3`
- **rust**: update toolchain and dependencies `31ec325e`

### 📊 Release Summary

**Total commits**: 8 across 4 categories

✨ **3** new features - *Enhanced functionality*
🔧 **1** improvement - *Better performance & code quality*
🐛 **2** bug fixes - *Improved stability*
🔄 **2** other changes - *Maintenance & tooling*

## [0.14.0] - 2025-09-16

### 📋 Release Summary

This release adds enhanced command hinting and completion, along with migration support for legacy session history, improving user workflow and continuity (bc3f31b5, 19e4057c). Pricing for DeepSeek has been updated to unified rates effective September 5, 2025 (f40aadca). Additionally, improvements to session history management and clearer error messages enhance overall usability and reliability (3c0afe96, b0d90a0e).


### ✨ New Features & Enhancements

- **session**: add hinting and completion for mor commands (/context, /mcp, /cache, /loglevel, /role, /model) `bc3f31b5`
- **deepseek**: update pricing to unified rates starting Sep 5, 2025 `f40aadca`
- **session**: add migration from legacy global history file `19e4057c`

### 🔧 Improvements & Optimizations

- **history**: implement role-based session history system `3c0afe96`

### 🐛 Bug Fixes & Stability

- **text_editing**: improve str_replace error message guidance `b0d90a0e`

### 📊 Release Summary

**Total commits**: 5 across 3 categories

✨ **3** new features - *Enhanced functionality*
🔧 **1** improvement - *Better performance & code quality*
🐛 **1** bug fix - *Improved stability*

## [0.13.0] - 2025-08-27

### 📋 Release Summary

This release adds new session commands and improved task management features, enhancing user interaction and planning capabilities (/run, /prompt, /plan) (61f22cda, e32c88be, 1be66add). Multimodal support and context handling are improved for smoother workflows (c33d653e, 8b3876de, 36263e31). Several bug fixes address Windows path handling, API cost tracking, and tool response errors, improving overall stability and usability (9a9c656e, 71aa15f1, dd410443). Documentation and installation guides have also been updated for better user guidance (d8426784, 9814b74e).


### ✨ New Features & Enhancements

- **session**: add completion and hints for /run and /prompt `61f22cda`
- **utils**: add context block detection and expansion `c33d653e`
- **chat**: add Ctrl+G to add message without sending `8b3876de`
- **mcp**: require tasks with title and description `e32c88be`
- **plan**: add /plan command to display current plan status `1be66add`
- **truncation**: add global MCP response tokens threshold `36263e31`

### 🔧 Improvements & Optimizations

- **file_parser**: add utilities for file reference extraction and rendering `041b8c46`

### 🐛 Bug Fixes & Stability

- **utils**: preserve Windows drive letters in file path parsing and rendering `9a9c656e`
- **utils**: normalize file paths on Windows in read_file_lines `92543735`
- **prompt**: prevent accidental continuation trigger on /prompt command `848f8293`
- **session**: preserve system message on /run command replace output `d78631c0`
- **session**: track API call cost immediately after response `71aa15f1`
- **cargo**: upgrade dependencies to fix cargo audit issues `6a144ece`
- **fs**: suggest line_replace for ambiguous replacements `3904a5a7`
- **mcp**: handle error flag in MCP tool responses correctly `dd410443`

### 📚 Documentation & Examples

- **config**: update configuration docs with command groups `d8426784`
- **utils**: add file parsing and rendering usage instructions `228c86bc`
- **installation**: rewrite installation guide `9814b74e`
- **plan**: enhance task description guidelines with examples `23b7934b`

### 🔄 Other Changes

- **file_renderer**: fix Windows test failures by normalizing paths and line endings `e57f5aba`
- **plan**: fix task format in plan tests after refactor `b829ef4f`

### 📊 Release Summary

**Total commits**: 21 across 5 categories

✨ **6** new features - *Enhanced functionality*
🔧 **1** improvement - *Better performance & code quality*
🐛 **8** bug fixes - *Improved stability*
📚 **4** documentation updates - *Better developer experience*
🔄 **2** other changes - *Maintenance & tooling*

## [0.12.0] - 2025-08-18

### 📋 Release Summary

This release adds reusable prompt templates and spending tracking to enhance session management, along with batch processing for improved tool efficiency (9848036f, f5b46206, 5a3926f3). Several bug fixes improve search accuracy, prevent retry loops, and enhance system stability (a24932e7, b9d4f783, 4edc057f). Dependency updates and testing refinements further optimize overall performance (a69f72b9, 74d80515).


### ✨ New Features & Enhancements

- **mcp,chat**: batch large output prompts for parallel tool calls `5a3926f3`
- **session**: add /prompt command with reusable templates `9848036f`
- **session**: add request spending threshold and tracking `f5b46206`

### 🐛 Bug Fixes & Stability

- **web**: clarify no-results issue with multiple quoted phrases `a24932e7`
- **style**: update format strings for Clippy compliance `01aaacf0`
- **list_files**: treat content pattern as fixed string to avoid regex errors `efb8a3b5`
- **session**: prevent infinite retry loop on continuation failure `b9d4f783`
- **mcp**: prevent recursion in cancellation polling loop `4edc057f`

### 🔄 Other Changes

- **deps**: update and reorganize dependencies `a69f72b9`
- **fs**: reset line count tracking in batch_edit test `74d80515`

### 📊 Release Summary

**Total commits**: 10 across 3 categories

✨ **3** new features - *Enhanced functionality*
🐛 **5** bug fixes - *Improved stability*
🔄 **2** other changes - *Maintenance & tooling*

## [0.11.0] - 2025-08-10

### 📋 Release Summary

This release adds support for the latest GPT-5 and Anthropic opus-4-1 AI models, expanding the range of AI capabilities (076e797d, b8f3576b). Several improvements enhance session stability and usability, including better input rendering with ANSI color support and cancellation options for agent tools (574d08e5, ed9e2820, e245ecff, a07109dd). Additionally, bug fixes prevent infinite retry loops and improve text editing reliability (574d08e5, 315ddfda).


### ✨ New Features & Enhancements

- **openai**: add GPT-5 model support with pricing and params `076e797d`
- **anthropic**: add opus-4-1 model and fix temp/top_p handling `b8f3576b`

### 🐛 Bug Fixes & Stability

- **session**: prevent infinite retries on continuation calls `574d08e5`
- **text_editing**: prevent repeated line_replace on new lines `315ddfda`
- **mcp**: add cancellation support to agent tool execution `ed9e2820`
- **session**: enable ANSI color mode for rustyline input rendering `e245ecff`
- **session**: prevent infinite retry loops on continuation errors `a07109dd`

### 📊 Release Summary

**Total commits**: 7 across 2 categories

✨ **2** new features - *Enhanced functionality*
🐛 **5** bug fixes - *Improved stability*

## [0.10.2] - 2025-07-26

### 📋 Release Summary

This release improves session reliability by addressing incomplete tool calls, message truncation, and session resuming issues (105fcd80, 2ba67733, a281b8d5, 54aed7c5, 0ace2654, d9ac1b30, 7b5dfada). Additionally, the /done command has been streamlined for better usability, and new quick start and troubleshooting guides have been added to enhance user onboarding (855d1898, 9872b47f).


### 🔧 Improvements & Optimizations

- **done**: move /done command to dedicated file and clean code `855d1898`

### 🐛 Bug Fixes & Stability

- **session**: detect and truncate earliest incomplete tool calls on r... `105fcd80`
- **session**: truncate messages on interrupted tool calls to clean state `2ba67733`
- **session**: correct tool_calls reconstruction on session resume `a281b8d5`
- **session**: restore layers state and cost on session resume `54aed7c5`
- **session**: handle incomplete tool calls in session resuming `0ace2654`
- **session**: re-add initial messages on /done command completion `d9ac1b30`
- **session**: prevent infinite loop on Ctrl+C cancellation `7b5dfada`

### 📚 Documentation & Examples

- **instructions**: add quick start and troubleshooting guide `9872b47f`

### 📊 Release Summary

**Total commits**: 9 across 3 categories

🔧 **1** improvement - *Better performance & code quality*
🐛 **7** bug fixes - *Improved stability*
📚 **1** documentation update - *Better developer experience*

## [0.10.1] - 2025-07-17

### 📋 Release Summary

This release addresses several session management issues to improve stability and user experience, including better handling of cancellation signals and cleanup of partial messages (61a235d4, 959310fb, 7e5ec3ea). Documentation has also been updated to reflect these enhancements.


### 🐛 Bug Fixes & Stability

- **session**: resolve cancellation issue and update documentation `61a235d4`
- **session**: handle cancellation signal correctly in tool loop `959310fb`
- **session**: clean up partial messages on tool and layer cancellation `7e5ec3ea`

### 📊 Release Summary

**Total commits**: 3 across 1 categories

🐛 **3** bug fixes - *Improved stability*

## [0.10.0] - 2025-07-15

### 📋 Release Summary

This release adds customizable output roles for session messages to enhance interaction control (ca1e288f). Several bug fixes improve session stability, including better cancellation handling, prevention of duplicate messages, and stricter configuration enforcement, alongside file lock implementation to avoid write conflicts and improved tool validation (93821f6, 6a9f835e, d25a7cd7, e2a05ccc, d2e707f9, e7277764, 5379398d). Additionally, the session flow has been streamlined for a smoother user experience (4091cf29).


### ✨ New Features & Enhancements

- **session**: add output_role control for session messages `ca1e288f`

### 🔧 Improvements & Optimizations

- **session**: simplify interactive and non-interactive session flow `4091cf29`

### 🐛 Bug Fixes & Stability

- **session**: propagate Ctrl+C cancellation to animation and tools `93821f6c`
- **mcp/fs**: add file locks to prevent concurrent write conflicts `6a9f835e`
- **config**: add missing output_role to default config sections `d25a7cd7`
- **session**: prevent duplicate user messages with layers and continu... `e2a05ccc`
- **session**: remove defaults to enforce strict output role config `d2e707f9`
- **session**: replace ctrlc crate with tokio signal for Ctrl+C handling `e7277764`
- **layers**: apply pattern-based tool validation in layers `5379398d`

### 📊 Release Summary

**Total commits**: 9 across 3 categories

✨ **1** new feature - *Enhanced functionality*
🔧 **1** improvement - *Better performance & code quality*
🐛 **7** bug fixes - *Improved stability*

## [0.9.0] - 2025-07-10

### 📋 Release Summary

This release introduces enhanced session management with dynamic role switching and customizable prompts and temperature settings for improved AI interactions (38ee56f2, 0bdaed2b, 5be4bbc5). Several bug fixes improve configuration stability and reliability, while additional tests ensure robust handling of batch edits (3111ac2f, 20301e74).


### ✨ New Features & Enhancements

- **config**: add top_k and top_p defaults and tune temperatures `38ee56f2`
- **config,ask,shell**: add configurable prompts and temperatures for... `0bdaed2b`
- **session**: add /role command to switch session role at runtime `5be4bbc5`

### 🐛 Bug Fixes & Stability

- **config**: use slice instead of Vec reference in show_mcp_servers `3111ac2f`

### 🔄 Other Changes

- **fs**: add critical batch_edit tests for line number handling `20301e74`

### 📊 Release Summary

**Total commits**: 5 across 3 categories

✨ **3** new features - *Enhanced functionality*
🐛 **1** bug fix - *Improved stability*
🔄 **1** other change - *Maintenance & tooling*

## [0.8.1] - 2025-07-03

### 📋 Release Summary

This release enhances session management with improved task finalization, streamlined continuation handling, and more reliable command processing (5523f0d, cf96448, 6ac8362, bb2f57a). User experience is improved through clearer guidance on parallel tool usage and more intuitive command completion (b328fda, 02e2025). Several bug fixes address token handling, logging clarity, and error reporting to ensure smoother and more stable interactions (5fc894a, fac0836, 2e24515, 58da6f3, ef4eb89).


### 🔧 Improvements & Optimizations

- **session**: simplify continuation logic and unify triggers `feb822c0`
- **session**: remove unused syntax highlighter methods and tests `93727213`

### 🐛 Bug Fixes & Stability

- **session**: update /done command message to "Finalizing current task `5523f0da`
- **session**: remove duplicated token limit log message `5fc894ac`
- **session**: use root max_retries instead of role config `fac08362`
- **session**: respect CLI options over config defaults for runtime pa... `2e245158`
- **session**: simplify continuation logic and avoid user prompt on to... `cf964480`
- **session**: handle continuation immediately to fix tool bug `6ac8362e`
- **session**: stop tool processing on continuation trigger `bb2f57a0`
- **session**: return error on invalid session token threshold at launch `58da6f35`
- **session**: run continuation check before tool calls evaluation `ef4eb893`
- **session**: enable bash-like completion with partial matches and re... `02e20255`

### 📚 Documentation & Examples

- **session**: clarify guidance on parallel tool usage in continuation `b328fdad`

### 📊 Release Summary

**Total commits**: 13 across 3 categories

🔧 **2** improvements - *Better performance & code quality*
🐛 **10** bug fixes - *Improved stability*
📚 **1** documentation update - *Better developer experience*

## [0.8.0] - 2025-07-02

### 📋 Release Summary

This release introduces enhanced session management with improved token tracking, continuation controls, and time monitoring, alongside expanded developer tools and multimodal support (6fabe56, b0ce4657, 3b81c7f). Configurable retry logic and updated pricing models improve AI provider integrations, while comprehensive documentation and testing bolster usability and reliability. Several bug fixes address token calculation accuracy, error handling, and session stability to ensure a smoother user experience (41090fc, c79cfb5f, f7270aee).


### ✨ New Features & Enhancements

- **session**: add role-based token counting and continuation checks `6fabe563`
- **session**: include system prompt and tools in context token count `b0ce4657`
- **session**: add flag to disable continuation triggers temporarily `174ca557`
- **deepseek**: update pricing scheme to use hash maps and helpers `d6ceb6e8`
- **dev**: add plan tool to developer MCP server `3b81c7fd`
- **session**: add API, tool, and total time tracking in layers `d5fcb59b`
- **fs**: support negative line ranges in text editor view_range `63778fed`
- **api**: add configurable retry logic for Amazon provider `2885fb13`
- **session**: preserve initial instructions and welcome messages on ... `880ed2d8`
- **batch_edit**: support single-file multiple operations with origin... `b497a2ca`

### 🔧 Improvements & Optimizations

- **providers**: unify Anthropic and OpenAI retry logic using ret... `ad111d0f`
- **agent,session**: source temperature from role config instead ... `d792da03`
- **batch_edit**: extract batch_edit as independent tool from tex... `2ac3c987`
- **ci**: fix markdown code block formatting in release workflow `0f2fde3e`

### 🐛 Bug Fixes & Stability

- **anthropic**: correct token usage calculation including cache tokens `41090fcd`
- **config**: track and display env var source including .env override `8f3805db`
- **session**: resolve OpenAI 400 errors and add CTRL-C cancellation `c79cfb5f`
- **plan**: prevent overwriting active plan on start command `4bd44337`
- **session**: correct continuation trigger timing in response processing `f513f964`
- **openai**: correct token cost calculation with cache tokens `10716178`
- **openai**: extract and set tool_call_id from response `1d76a461`
- **chat**: use correct model and params for auto threshold continuation `f7270aee`

### 📚 Documentation & Examples

- **mcp**: add detailed docs for new plan tool and usage `f597b7c0`
- **doc**: actualize installation, overview, and config guides `8dc57fce`
- specify cargo check with short message format in instructions `24862099`

### 🔄 Other Changes

- **plan**: fix async test assertions and add serial execution `d44b768a`
- **plan**: add comprehensive tests for plan tool commands `3f49e94a`
- **anthropic**: update model pricing to June 2025 rates `4f558d96`
- **openai**: update model list and pricing to 2025 versions `e9f0841f`

### 📊 Release Summary

**Total commits**: 29 across 5 categories

✨ **10** new features - *Enhanced functionality*
🔧 **4** improvements - *Better performance & code quality*
🐛 **8** bug fixes - *Improved stability*
📚 **3** documentation updates - *Better developer experience*
🔄 **4** other changes - *Maintenance & tooling*

## [Unreleased] - 2025-07-01

### 🐛 Critical Bug Fixes

- **session**: fix OpenAI API 400 errors during parallel tool execution `[current]`
  - Fixed session continuation triggering mid-tool processing causing incomplete tool_calls/tool_results mapping
  - Added `TruncationOptions` struct with `defer_continuation` flag for clean API
  - Implemented proper cancellation support with CTRL-C handling during continuation operations
  - Refactored duplicate functions and eliminated code smells following senior developer practices

### 🔧 Code Quality Improvements

- **context_truncation**: eliminate duplicate functions and parameter pollution `[current]`
  - Replaced `check_and_truncate_context_with_defer` with clean options pattern
  - Removed unused `_role` and `_operation_cancelled` parameters
  - Added proper `TruncationOptions` struct following Rust best practices
  - Implemented cancellation-aware versions with `Option<Arc<AtomicBool>>` pattern

### ✨ New Features

- **cancellation**: add CTRL-C support to session continuation system `[current]`
  - Users can now interrupt long-running summarization operations
  - Added `check_and_truncate_context_with_cancellation` function
  - Added `check_and_handle_continuation_with_cancellation` function
  - Integrated with existing cancellation infrastructure from runner.rs

## [0.7.0] - 2025-06-27

### 📋 Release Summary

This release introduces a new static website for the Octomind project and enhances user experience with improved session continuity and thread-safe history management (d964df21, 7039b455, b06e67b3). Several bug fixes address error handling, input validation, and stability across core features, while dependency updates and documentation improvements further refine overall usability (13e984c4, 52144d2a, 86e45d84, 445db4ca, 049062bd).


### ✨ New Features & Enhancements

- **docs**: add /run command and update continuation architecture docs `e0ff1873`
- **ask**: add separate thread-safe history file for ask mode `b06e67b3`
- **ast_grep**: replace glob crate with ignore for glob expansion `b55fd02f`
- **website**: add complete static site for Octomind project `d964df21`

### 🔧 Improvements & Optimizations

- **fs**: unify error response creation with McpToolResult::error `3dcaf6ed`
- **fs**: make ripgrep line parsing UTF-8 safe `29a74f07`
- **session**: replace manual context limit prompt with auto cont... `564c3423`
- **session**: improve extract_lines output and truncate logic `f2d8c17c`
- **continuation**: extract session summary prompt constants and ... `5abed7d8`

### 🐛 Bug Fixes & Stability

- **text_editing**: add validation and clarify escaped chars error mes... `13e984c4`
- **session**: ensure continuation processes after tool calls complete `7039b455`
- **truncation**: prevent crash by handling char boundaries safely `ab158d51`
- **mcp**: return structured errors for invalid params and cancellations `52144d2a`
- **fs**: return structured errors for invalid parameters and missing ... `86e45d84`
- **commands**: simplify error handling in interactive input `445db4ca`
- **ci**: correct preview URL and remove Lighthouse job `ecda5e8b`
- **install**: correct install script and update master branch path `049062bd`

### 📚 Documentation & Examples

- **fs**: clarify raw content uses actual whitespace, not escapes `3c96628f`
- **session**: update docs to reflect modular continuation refactor `8e53f517`
- **ast_grep**: expand pattern syntax with anonymous wildcard and exa... `8e2d6125`

### 🔄 Other Changes

- **deps**: upgrade html5ever and related crates to latest versions `2ea24fbd`
- **deps**: upgrade multiple dependencies to latest versions `b0be0953`

### 📊 Release Summary

**Total commits**: 22 across 5 categories

✨ **4** new features - *Enhanced functionality*
🔧 **5** improvements - *Better performance & code quality*
🐛 **8** bug fixes - *Improved stability*
📚 **3** documentation updates - *Better developer experience*
🔄 **2** other changes - *Maintenance & tooling*

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
