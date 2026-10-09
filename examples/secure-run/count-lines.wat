;; Copies input.txt to out.txt one byte at a time and logs the number of lines.
;;
;; The manifest grants one read (handle 0, input.txt) and one write (handle 0, out.txt).
;; The script returns 0, so out.txt is written when the run completes.
(module
  (import "tpt" "log" (func $log (param i64)))
  (import "tpt" "fs_open_read" (func $open (param i32) (result i32)))
  (import "tpt" "fs_size" (func $size (param i32) (result i64)))
  (import "tpt" "fs_byte" (func $byte (param i32 i64) (result i32)))
  (import "tpt" "fs_write_byte" (func $write (param i32 i32) (result i32)))

  (func (export "run") (result i32)
    (local $fd i32)
    (local $len i64)
    (local $at i64)
    (local $b i32)
    (local $lines i64)

    (local.set $fd (call $open (i32.const 0)))
    (local.set $len (call $size (local.get $fd)))

    (block $done
      (loop $next
        (br_if $done (i64.ge_s (local.get $at) (local.get $len)))
        (local.set $b (call $byte (local.get $fd) (local.get $at)))
        (if (i32.eq (local.get $b) (i32.const 10))
          (then (local.set $lines (i64.add (local.get $lines) (i64.const 1)))))
        (drop (call $write (i32.const 0) (local.get $b)))
        (local.set $at (i64.add (local.get $at) (i64.const 1)))
        (br $next)))

    (call $log (local.get $lines))
    (i32.const 0)))
