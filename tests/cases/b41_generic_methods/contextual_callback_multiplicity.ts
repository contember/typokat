// B41 regression — contextual generic callback replay must not duplicate the
// pre-existing backlog-75 false positive. tsc 6.0.3 --strict is clean here; one
// TK2345 per call is the documented B75 over-report, while this fixture owns only
// the duplicate emitted after contextual callback typing.

interface Events {
  text: [text: string]; // incomplete[annotation-lower/named-tuple-member/self]
  count: [count: number]; // incomplete[annotation-lower/named-tuple-member/self]
}

declare class Client {
  on<K extends keyof Events>(key: K, callback: (...args: Events[K]) => void): void;
}

declare const client: Client;

client.on("text", value => {}); // error[TK2345]
client.on("count", value => {}); // error[TK2345]
