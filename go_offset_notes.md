# Notes

## What do we need to build?

- Really, all we need is a unidirecitonal iterator
  - TODO: check which bits of the flume api patchql uses

## How would we iterate over a go offset?

- Open all three files.

- Check the journal
  - (how do we respect locking?)
  - the journal gives us the number or entries in the log
- Start iterating the offset file
  - map the offset file number to size.
  - map the size to a slice of bytes by reading that size from the file.

Actually, that's not even needed right?

- all we do is open the data file, read a u64 as the size, then read that many bytes. Repeat.

