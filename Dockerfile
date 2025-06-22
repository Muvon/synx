# Multi-stage Dockerfile for octomind
# Stage 1: Build
FROM rust:1.87-slim AS builder

# Install system dependencies
RUN apt-get update && apt-get install -y \
		pkg-config \
		libssl-dev \
		&& rm -rf /var/lib/apt/lists/*

# Create app directory
WORKDIR /app

# Copy manifests
COPY Cargo.toml Cargo.lock ./

# Copy source code and config templates
COPY src ./src
COPY config-templates ./config-templates

# Build the application
RUN cargo build --release

# Stage 2: Runtime
FROM debian:bookworm-slim

# Install runtime dependencies and development tools
RUN apt-get update && apt-get install -y \
		ca-certificates \
		curl \
		wget \
		ripgrep \
		&& rm -rf /var/lib/apt/lists/* \
		&& update-ca-certificates

# Install ast-grep (sg) from GitHub releases
RUN curl -L https://github.com/ast-grep/ast-grep/releases/latest/download/ast-grep-x86_64-unknown-linux-gnu.tar.gz | tar xz \
		&& mv ast-grep-x86_64-unknown-linux-gnu/sg /usr/local/bin/ \
		&& rm -rf ast-grep-x86_64-unknown-linux-gnu

# Install octocode from GitHub releases
RUN curl -L https://github.com/muvon/octocode/releases/latest/download/octocode-x86_64-unknown-linux-gnu.tar.gz | tar xz \
		&& mv octocode /usr/local/bin/ \
		&& chmod +x /usr/local/bin/octocode

# Create a non-root user
RUN groupadd -r octomind && useradd -r -g octomind octomind

# Create app directory
WORKDIR /app

# Copy the binary from builder stage
COPY --from=builder /app/target/release/octomind /usr/local/bin/octomind

# Change ownership to non-root user
RUN chown -R octomind:octomind /app

# Switch to non-root user
USER octomind

# Expose port (if applicable)
# EXPOSE 8080

# Health check (customize based on your application)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
		CMD octomind --help || exit 1

# Set the entrypoint
ENTRYPOINT ["octomind"]
CMD ["--help"]
