# Container image for TPT App Policy (spec §25).
#
# Not yet built or tested: Docker was not available when this was written.
#
# Build:  docker build -t tpt/app-policy .
# Run a policy as an HTTP service, reachable from the host:
#   docker run --rm -p 8080:8080 -e TPT_POLICY_TOKEN=change-me \
#     -v "$PWD/policies:/data:ro" tpt/app-policy \
#     serve /data/expense.yaml --listen 0.0.0.0:8080
# Run the CLI on a file:
#   docker run --rm -v "$PWD:/data:ro" tpt/app-policy check /data/policy.yaml /data/input.json

FROM rust:1-bookworm AS build
WORKDIR /src
COPY . .
RUN cargo build --release -p tpt-app-policy

FROM debian:bookworm-slim
RUN useradd --system --uid 10001 tpt
COPY --from=build /src/target/release/tpt-policy /usr/local/bin/tpt-policy
USER tpt
WORKDIR /data
EXPOSE 8080
ENTRYPOINT ["tpt-policy"]
CMD ["--help"]
