# Complex formatting test
let data = {
  name: "test",
  value: 42,
}

let list = [
  1,
  2,
  3,
]

let closure = {|x,y| $x + $y }

$data | get name | str upcase

if true {
  if false {
    echo "nested"
  }
}
