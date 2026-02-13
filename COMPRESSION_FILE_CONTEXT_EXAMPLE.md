# Compression File Context - Visual Example

## What AI Provides in Compression Summary

When compression triggers, AI can now request specific file contexts:

```
YES
User was implementing authentication flow with JWT tokens. Key decisions: using RS256 algorithm, 15-minute token expiry, refresh token rotation. Implementation is 80% complete with login endpoint working but logout needs session cleanup.

<context>
src/auth/jwt.rs:45:120
src/auth/handlers.rs:200:250
src/config/auth.rs:10:30
</context>
```

## What Gets Auto-Expanded and Injected

The system automatically:
1. **Parses** `<context>` tags using `parse_file_contexts()` (same as continuation)
2. **Reads** actual file content using `read_file_lines()`
3. **Renders** as XML using `render_files_as_xml()` (same as continuation)
4. **Injects** into compressed message

## Final Compressed Message Format

```markdown
## Conversation Summary [COMPRESSED: abc123]

**CONTEXT**: User was implementing authentication flow with JWT tokens. Key decisions: using RS256 algorithm, 15-minute token expiry, refresh token rotation. Implementation is 80% complete with login endpoint working but logout needs session cleanup.

**FILE CONTEXT** (auto-expanded):
FILE CONTEXT:

<content path="src/auth/jwt.rs" lines="45:120">
45: pub struct JwtManager {
46:     private_key: RsaPrivateKey,
47:     public_key: RsaPublicKey,
48:     token_expiry: Duration,
49: }
50:
51: impl JwtManager {
52:     pub fn new(config: &AuthConfig) -> Result<Self> {
53:         let private_key = load_private_key(&config.key_path)?;
54:         let public_key = RsaPublicKey::from(&private_key);
55:
56:         Ok(Self {
57:             private_key,
58:             public_key,
59:             token_expiry: Duration::from_secs(900), // 15 minutes
60:         })
61:     }
62:
63:     pub fn generate_token(&self, user_id: &str) -> Result<String> {
64:         let claims = Claims {
65:             sub: user_id.to_string(),
66:             exp: (Utc::now() + self.token_expiry).timestamp(),
67:             iat: Utc::now().timestamp(),
68:         };
69:
70:         encode(&Header::new(Algorithm::RS256), &claims, &self.private_key)
71:             .map_err(|e| anyhow!("Failed to encode JWT: {}", e))
72:     }
73:
74:     pub fn verify_token(&self, token: &str) -> Result<Claims> {
75:         decode::<Claims>(
76:             token,
77:             &DecodingKey::from_rsa_components(&self.public_key),
78:             &Validation::new(Algorithm::RS256),
79:         )
80:         .map(|data| data.claims)
81:         .map_err(|e| anyhow!("Invalid token: {}", e))
82:     }
83: }
...
120: }
</content>

<content path="src/auth/handlers.rs" lines="200:250">
200: pub async fn login_handler(
201:     State(state): State<AppState>,
202:     Json(payload): Json<LoginRequest>,
203: ) -> Result<Json<LoginResponse>, ApiError> {
204:     // Validate credentials
205:     let user = state.db
206:         .get_user_by_email(&payload.email)
207:         .await?
208:         .ok_or(ApiError::Unauthorized)?;
209:
210:     if !verify_password(&payload.password, &user.password_hash)? {
211:         return Err(ApiError::Unauthorized);
212:     }
213:
214:     // Generate tokens
215:     let access_token = state.jwt_manager.generate_token(&user.id)?;
216:     let refresh_token = generate_refresh_token();
217:
218:     // Store refresh token with rotation
219:     state.db.store_refresh_token(&user.id, &refresh_token).await?;
220:
221:     Ok(Json(LoginResponse {
222:         access_token,
223:         refresh_token,
224:         expires_in: 900,
225:     }))
226: }
227:
228: pub async fn logout_handler(
229:     State(state): State<AppState>,
230:     Extension(user_id): Extension<String>,
231: ) -> Result<StatusCode, ApiError> {
232:     // TODO: Implement session cleanup
233:     // Need to:
234:     // 1. Invalidate refresh token
235:     // 2. Add access token to blacklist (until expiry)
236:     // 3. Clear any cached user data
237:
238:     Ok(StatusCode::OK)
239: }
...
250: }
</content>

<content path="src/config/auth.rs" lines="10:30">
10: #[derive(Debug, Clone, Deserialize)]
11: pub struct AuthConfig {
12:     pub key_path: PathBuf,
13:     pub token_expiry_secs: u64,
14:     pub refresh_token_expiry_days: u64,
15:     pub algorithm: JwtAlgorithm,
16: }
17:
18: #[derive(Debug, Clone, Deserialize)]
19: pub enum JwtAlgorithm {
20:     RS256,
21:     RS384,
22:     RS512,
23: }
24:
25: impl Default for AuthConfig {
26:     fn default() -> Self {
27:         Self {
28:             key_path: PathBuf::from("keys/private.pem"),
29:             token_expiry_secs: 900, // 15 minutes
30:             refresh_token_expiry_days: 30,
...
</content>

**Compression Info**:
- ID: `abc123`
- Type: Semantic compression with file context
---
*Compressed using importance-based semantic chunking with automatic file context expansion.*
```

## Key Benefits

### 1. **Same System as Continuation**
- Uses identical `parse_file_contexts()` function
- Uses identical `generate_file_context_content()` function
- Uses identical `render_files_as_xml()` function
- **Zero code duplication** - just reuses proven continuation logic

### 2. **Clear XML Format**
```xml
<content path="src/auth/jwt.rs" lines="45:120">
[actual file content with line numbers]
</content>
```

- **Structured**: AI can easily parse and reference
- **Explicit**: Path and line range clearly visible
- **Escaped**: XML-safe (handles special characters)
- **Readable**: Clean format for AI consumption

### 3. **AI Won't Re-Read Files**
The file content is **already in context** as part of the compressed message:

❌ **Before** (without file context):
```
AI: "I need to check src/auth/jwt.rs to continue"
AI: *calls text_editor view src/auth/jwt.rs*
AI: *reads 500 lines*
AI: *finds relevant section*
```

✅ **After** (with file context):
```
AI: "I can see the JWT implementation in the file context"
AI: *directly references the code already in context*
AI: *continues work immediately*
```

### 4. **Token Efficient**
- AI only requests **critical files** (max 5 ranges)
- Only **specific line ranges** (not entire files)
- Compressed summary + file context < original conversation
- Still saves tokens overall

## Comparison with Continuation

| Feature | Continuation | Compression |
|---------|-------------|-------------|
| **Trigger** | Token limit reached | Conversation gets repetitive |
| **Purpose** | Start fresh session | Compress older exchanges |
| **File Context** | ✅ Yes (required) | ✅ Yes (optional) |
| **Parsing** | `parse_file_contexts()` | `parse_file_contexts()` ✅ Same |
| **Rendering** | `render_files_as_xml()` | `render_files_as_xml()` ✅ Same |
| **Format** | `<context>` tags | `<context>` tags ✅ Same |
| **Max Files** | 10 ranges | 5 ranges (more conservative) |

## Example Flow

```
1. Compression triggers (49 messages → compress)
   ↓
2. AI analyzes conversation chunks
   ↓
3. AI decides: "YES, compress with file context"
   ↓
4. AI provides summary + <context> tags
   ↓
5. System parses: parse_file_contexts(summary)
   ↓
6. System reads: read_file_lines(filepath, range)
   ↓
7. System renders: render_files_as_xml(contents)
   ↓
8. System injects: format_compressed_entry_with_context()
   ↓
9. Compressed message now contains:
   - Summary of what happened
   - Critical file content (auto-expanded)
   ↓
10. AI continues work with full context
    (no need to re-read files)
```

## Code Reuse Verification

```rust
// In apply_compression() - src/session/chat/conversation_compression.rs:690-702

// Parse file contexts (SAME as continuation)
let file_contexts = super::continuation::file_context::parse_file_contexts(context_summary);

// Generate file content (SAME as continuation)
let file_context_content = super::continuation::file_context::generate_file_context_content(&file_contexts);

// Result: Identical XML output format as continuation
```

✅ **100% code reuse** - no duplication, proven logic, consistent format
