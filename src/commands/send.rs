// Copyright 2025 Muvon Un Limited
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! `octomind inject` — send a message to a running session via its Unix Domain Socket.

use anyhow::{bail, Context, Result};
use clap::Args;
use std::io::{self, Read};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

#[derive(Args, Debug)]
pub struct InjectArgs {
	/// Name of the running session to inject into.
	#[arg(value_name = "SESSION")]
	pub session: String,

	/// Message to inject. If omitted, reads from stdin.
	#[arg(value_name = "MESSAGE")]
	pub message: Option<String>,
}

pub async fn execute(args: &InjectArgs) -> Result<()> {
	let message = match &args.message {
		Some(m) => m.clone(),
		None => {
			let mut buf = String::new();
			io::stdin()
				.read_to_string(&mut buf)
				.context("failed to read message from stdin")?;
			buf.trim().to_string()
		}
	};

	if message.is_empty() {
		bail!("message must not be empty");
	}

	let sock_path = octomind::directories::get_run_dir()
		.context("failed to resolve run directory")?
		.join(format!("{}.sock", args.session));

	if !sock_path.exists() {
		bail!(
			"no running session named '{}' (socket not found at {:?})",
			args.session,
			sock_path
		);
	}

	let mut stream = UnixStream::connect(&sock_path)
		.await
		.with_context(|| format!("failed to connect to session '{}'", args.session))?;

	// Send message then shut down write half so the session knows we're done.
	stream
		.write_all(message.as_bytes())
		.await
		.context("failed to send message")?;
	stream
		.shutdown()
		.await
		.context("failed to shut down write half")?;

	// Read the response: "ok\n" or "error: ...\n"
	let mut response = String::new();
	stream
		.read_to_string(&mut response)
		.await
		.context("failed to read response")?;

	let response = response.trim();
	if response == "ok" {
		println!("Injected into session '{}'.", args.session);
		Ok(())
	} else {
		bail!("session returned: {}", response);
	}
}
