FROM mcr.microsoft.com/devcontainers/base:ubuntu

# Install system dependencies for Smithay / Wayland compositor development
RUN apt-get update && apt-get install -y --no-install-recommends \
    build-essential \
    pkg-config \
    cmake \
    # Wayland libraries
    libwayland-dev \
    libwayland-client0 \
    libwayland-server0 \
    wayland-protocols \
    # Input handling
    libinput-dev \
    libudev-dev \
    libseat-dev \
    libxkbcommon-dev \
    # Winit backend (EGL / rendering)
    libegl-dev \
    libgles2-mesa-dev \
    libgbm-dev \
    libdrm-dev \
    # X11 / XWayland support
    libxcb1-dev \
    libx11-xcb-dev \
    # Miscellaneous
    libdbus-1-dev \
    libsystemd-dev \
    libpixman-1-dev \
    npm

# Install Gemini CLI
RUN npm install -g @google/gemini-cli

USER vscode

# Install Claude Code CLI
RUN (cd ~ && (curl -fsSL https://claude.ai/install.sh | bash))