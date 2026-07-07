// backlog 53: assignments inside switch clause bodies must reach the post-switch
// join and the fallthrough entry of the next clause. Silent at the buggy HEAD.
function s1(x: string | null, k: "a" | "b") {
  if (x === null) return;
  switch (k) {
    case "a":
      x = null;
      break;
  }
  const y: string = x; // error[TK2322]
}
function s3(x: string | null, k: "a" | "b") {
  if (x === null) return;
  switch (k) {
    case "a":
      x = null;
    case "b": {
      const y: string = x; // error[TK2322]
      break;
    }
  }
}
