// B41 regression — named tuple labels are supported. Empty callbacks do not
// observe their payload and are tsc-clean; the stronger selected-payload backlog-75
// witness remains in sr_deferred_ledger/b75_generic_indexed_access.ts.

interface Events {
  text: [text: string];
  count: [count: number];
}

declare class Client {
  on<K extends keyof Events>(key: K, callback: (...args: Events[K]) => void): void;
}

declare const client: Client;

client.on("text", value => {});
client.on("count", value => {});
