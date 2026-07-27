// Backlog 102, the headline case. A script `.ts` file declares an ordinary global TYPE. Nothing
// in the default library is called `B102CrossShape`, so this is a fresh name and no collision
// route is involved; the delta simply needs a global scope it is allowed to write to.
interface B102CrossShape {
  name: string;
}
