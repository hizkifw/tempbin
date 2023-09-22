FROM rust:alpine3.18 AS builder

WORKDIR /opt/app
RUN apk add alpine-sdk

COPY . .
RUN cargo build --release

FROM alpine:3.18.2

EXPOSE 1337
ENV LISTEN='0.0.0.0:1337'
WORKDIR /opt/app
RUN mkdir uploads

COPY --from=builder /opt/app/target/release/tempbin /opt/app/tempbin

CMD /opt/app/tempbin
