// B41 regression — named tuple labels are supported, while the pre-existing
// backlog-75 TK2345 false positive remains exactly once per call (never duplicated).

interface Events {
  text: [text: string];
  count: [count: number];
}

declare class Client {
  on<K extends keyof Events>(key: K, callback: (...args: Events[K]) => void): void;
}

declare const client: Client;

client.on("text", value => {}); // error[TK2345]
client.on("count", value => {}); // error[TK2345]
