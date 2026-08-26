FROM ubuntu:24.04@sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea

LABEL org.opencontainers.image.base.name="docker.io/library/ubuntu:24.04"
LABEL org.opencontainers.image.base.digest="sha256:561618e2c15bf2397621dd04f96926663a3b5616c189cf7e38db7e82f5c538ea"
LABEL io.verisilo.base.linux-amd64-manifest="sha256:019e8eb29a85e74d64925745884f2ec79aa27e3feab36353d24656f4d6b89467"
LABEL io.verisilo.recipe="camoufox-152.0.4-beta.28-r1-formal-v3"

ARG DEBIAN_FRONTEND=noninteractive

ENV HOME=/build-home \
    LANG=C.UTF-8 \
    LC_ALL=C.UTF-8 \
    MOZBUILD_STATE_PATH=/build-home/.mozbuild \
    PATH=/build-home/.cargo/bin:/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin \
    TZ=Etc/UTC

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        aria2 \
        bash \
        build-essential \
        ca-certificates \
        ccache \
        clang-18 \
        curl \
        file \
        findutils \
        git \
        golang-go \
        gzip \
        libc6-i386=2.39-0ubuntu8.8 \
        libsqlite3-dev \
        lld-18 \
        make \
        msitools \
        p7zip-full \
        patch \
        perl \
        python3 \
        python3-dev \
        python3-pip \
        python3-venv \
        rsync \
        rustc \
        tar \
        unzip \
        wget \
        xz-utils \
        zip \
    && test "$(dpkg-query -W -f='${Architecture}=${Version}' libc6-i386)" = "amd64=2.39-0ubuntu8.8" \
    && test "$(dpkg-query -W -f='${Architecture}=${Version}' libc6)" = "amd64=2.39-0ubuntu8.8" \
    && test -x /lib/ld-linux.so.2 \
    && test -z "$(dpkg --print-foreign-architectures)" \
    && update-alternatives --install /usr/bin/ld.lld ld.lld /usr/bin/ld.lld-18 100 \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /build-home /inputs /out /work

COPY strict_build.py /usr/local/bin/verisilo-r1-formal-strict-build

ENTRYPOINT ["python3", "/usr/local/bin/verisilo-r1-formal-strict-build"]
