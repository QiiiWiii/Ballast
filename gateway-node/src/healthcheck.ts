import * as grpc from "@grpc/grpc-js";

import { marketDataService } from "./grpc/proto.js";

const client = new marketDataService(
  "127.0.0.1:50051",
  grpc.credentials.createInsecure(),
);

client.health({}, (error, response) => {
  client.close();
  if (error || !response) {
    process.exitCode = 1;
  }
});
