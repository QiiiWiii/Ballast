FROM node:22-bookworm-slim AS builder

WORKDIR /app/web
COPY web/package.json web/package-lock.json ./
RUN npm ci
COPY web ./
RUN npm run build

FROM nginx:1.29-alpine
COPY deploy/nginx.conf /etc/nginx/conf.d/default.conf
COPY deploy/web-entrypoint.sh /docker-entrypoint.d/40-ballast-config.sh
COPY --from=builder /app/web/dist /usr/share/nginx/html
EXPOSE 80
HEALTHCHECK --interval=5s --timeout=3s --start-period=5s --retries=12 CMD wget --no-verbose --tries=1 --spider http://127.0.0.1/health || exit 1
