FROM rust:alpine3.18 as BUILDER
WORKDIR /opt/app
COPY . .

RUN apk add alpine-sdk
RUN cargo build --release

FROM alpine:3.18.2

EXPOSE 1337
ENV LISTEN='0.0.0.0:1337'
WORKDIR /opt/app

COPY --from=BUILDER /opt/app/target/release/tempbin /opt/app/tempbin
RUN mkdir uploads
CMD /opt/app/tempbin
