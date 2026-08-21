FROM node:22-bookworm-slim AS builder

WORKDIR /app/gateway-node
COPY gateway-node/package.json gateway-node/package-lock.json ./
RUN npm ci
COPY gateway-node ./
COPY proto /app/proto
RUN npm run build

FROM node:22-bookworm-slim AS runtime

ENV NODE_ENV=production
WORKDIR /app/gateway-node
COPY gateway-node/package.json gateway-node/package-lock.json ./
RUN npm ci --omit=dev
COPY --from=builder /app/gateway-node/dist ./dist
COPY proto /app/proto

USER node
EXPOSE 50051
HEALTHCHECK --interval=5s --timeout=3s --start-period=5s --retries=12 CMD ["node", "dist/healthcheck.js"]
ENTRYPOINT ["node", "dist/index.js"]
