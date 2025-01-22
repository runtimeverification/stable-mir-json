module Main (main) where

import Data.Aeson qualified as Json
import Data.ByteString.Lazy qualified as BS
import Data.Text.IO qualified as Text
import System.Environment

import Data.Smir.Json ()
import Data.Smir.Cfg
import Data.Smir.Construct

main :: IO ()
main = getArgs >>= mapM_ process

process :: FilePath -> IO ()
process file = do
    BS.readFile file >>=
        either error (pure . render . construct) . Json.eitherDecode >>=
        Text.writeFile (file <> ".dot")
