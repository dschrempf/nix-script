#!nix-script
#!buildInputs haskellPackages.ghc
#!build ghc -O -o $OUT_FILE $SCRIPT_FILE

main :: IO ()
main = putStrLn "Hello, World!"