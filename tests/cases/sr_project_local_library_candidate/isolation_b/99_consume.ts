const ownB: readonly number[] = [1].projectBOnly();
[1].projectAOnly(); // error[TK2551]: Property 'projectAOnly' does not exist on type 'number[]'. Did you mean 'projectBOnly'?
