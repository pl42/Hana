FROM fedora:36 as builder

# base requirements
RUN dnf install -y git clang \
    cmake e2fsprogs e2fsprogs-devel \
    protobuf-compiler protobuf-devel

RUN mkdir /rust
WORKDIR /rust

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs > install_rust.sh
RUN chmod +x ./install_rust.sh
RUN ./install_rust.sh -y
ENV PATH="${PATH}:/root/.cargo/bin"

WORKDIR /hana
ADD . .

RUN cargo build --workspace --profile=production

FROM fedora:36
RUN dnf install -y e2fsprogs

# Avoid copying over a bunch of junk
COPY --from=builder /hana/target/production/hana /usr/local/bin/hana
COPY --from=builder /hana/target/production/hana-rpc /usr/local/bin/hana-rpc
COPY --from=builder /hana/target/production/hana-sentry /usr/local/bin/hana-sentry
COPY --from=builder /hana/target/production/hana-toolbox /usr/local/bin/hana-toolbox
COPY --from=builder /hana/target/production/consensus-tests /usr/local/bin/consensus-tests

ARG UID=1000
ARG GID=1000

RUN groupadd -g $GID hana
RUN adduser --uid $UID --gid $GID hana
USER hana
RUN mkdir -p ~/.local/share/hana

EXPOSE 8545 \
    8551 \
    30303 \
    30303/udp \
    7545
