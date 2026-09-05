// Any write to a participant disables correlation for the whole callback, not just later reads.

type WritableContextRows = (...args: ["A", number] | ["B", string]) => void;

const laterPayloadWrite: WritableContextRows = (kind, payload) => {
  if (kind === "A") payload.toFixed(); // error[TK2339]
  payload = "changed";
};
const earlierKindWrite: WritableContextRows = (kind, payload) => {
  kind = "A";
  if (kind === "A") payload.toFixed(); // error[TK2339]
};
const capturedPayloadWrite: WritableContextRows = (kind, payload) => {
  if (kind === "A") payload.toFixed(); // error[TK2339]
  const changePayload = () => { payload = "changed"; };
};
const selfPayloadWrite: WritableContextRows = (kind, payload) => {
  if (kind === "A") payload.toFixed(); // error[TK2339]
  payload = payload;
};
const unrelatedLocalWrite: WritableContextRows = (kind, payload) => {
  let local = 0;
  local = 1;
  if (kind === "A") payload.toFixed();
};
const shadowedNestedWrite: WritableContextRows = (kind, payload) => {
  const changeOtherPayload = (payload: string) => { payload = "changed"; };
  if (kind === "A") payload.toFixed();
};
const defaultedDiscriminant: WritableContextRows = (kind = "A", payload) => {
  if (kind === "A") payload.toFixed(); // error[TK2339]
};
const payloadGuardDoesNotSelectKind: WritableContextRows = (kind, payload) => {
  if (typeof payload === "number") {
    const kindRemainsAUnion: "A" = kind; // error[TK2322]
    payload.toFixed();
  }
};
