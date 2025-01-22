module Data.Smir.Cfg (
  -- TODO
  module Data.Smir.Cfg,
) where

import Data.Hashable (hash)
import Data.Map.Strict (Map)
import Data.Map.Strict qualified as Map
import Data.Maybe (catMaybes)
import Data.Text (Text)
import Data.Text qualified as Text
import Numeric (showHex)

-- items as subgraph (node cluster of blocks)
data CfgItem = CfgItem
    { humanName :: Text
    , blocks :: [CfgBlock]
    -- , attributes :: ... to determine colour etc
    }
    deriving (Eq, Show)

-- | one node in an item cluster
-- may have one edge to another cluster (call) and internal next block edges
data CfgBlock = CfgBlock
    { callEdge :: Maybe CallEdge -- ^ out-edge and labels
    , innerEdges :: [NextEdge] -- ^ edges within the cluster
    , terminator :: CfgTerminator -- ^ Label on the block
    {- , statements :: [CfgStmt] -- ^ straight-line statements in block -}
    }
    deriving (Eq, Show)

data CfgTerminator
    = Call
    | Drop
    | Assert
    | Goto
    | SwitchInt
    | Resume
    | Return
    | Unreachable
    deriving (Eq, Show)

data CallEdge = CallEdge
    { callee :: Text -- ^ resolved using function map, or else number
                     -- in ty field (if not present)
    , label :: Text -- [Operand]
    }
    deriving (Eq, Show)

data NextEdge = NextEdge
    { label :: Maybe Text
    , next :: Int
    }
    deriving (Eq, Show)

data Cfg = Cfg
    { name :: Text
    , items :: Map Text CfgItem
    }
    deriving (Eq, Show)

---------------------------------------------------
-- rendering the graph

render :: Cfg -> Text
render Cfg{name, items} =
    "digraph \"" <> name <> "\" " <> inCurly (options <> "\n" <> renderItems items)
  where
    options = "node [shape = box]"


inCurly :: Text -> Text
inCurly contents = "{\n" <> indent 2 contents <> "\n}\n"

indent :: Int -> Text -> Text
indent n = Text.unlines . map (space <>) . Text.lines
    where space = Text.replicate n " "

renderItems :: Map Text CfgItem -> Text
renderItems mapping =
    Text.unlines $ map (uncurry renderItem) $ Map.assocs mapping
    where
      renderItem :: Text -> CfgItem -> Text
      renderItem longName item =
          "subgraph cluster_" <> short <> " "
             <> inCurly (Text.unlines $ [ "label=\"" <> withBreaks 26 item.humanName <> "\""
                                      -- colour and other attributes...
                                        , "style=filled"
                                        , "color=" <> if nonLocal then "lightyellow" else "lightblue"
                                        ] <> zipWith renderBlock [0..] item.blocks
                        )
             <> callEdges item.blocks
        where
          nonLocal = "::" `Text.isInfixOf` item.humanName

          short = shortName longName

          renderBlock :: Int -> CfgBlock -> Text
          renderBlock n b =
              let name = blockName longName n
              in Text.unlines $
                     (name <> " [ label = \"" <> Text.pack (show b.terminator) <> "\" ]")
                     : [ name <> " -> " <> blockName longName e.next
                         <> maybe "" (\l -> " [ label = \"" <> l <> "\" ]") e.label
                       | e <- b.innerEdges]

          callEdges :: [CfgBlock] -> Text
          callEdges =
              Text.unlines . catMaybes . zipWith callEdgeFrom [0..]

          callEdgeFrom :: Int -> CfgBlock -> Maybe Text
          callEdgeFrom n block
              | Just e <- block.callEdge =
                    let edge = blockName longName n
                               <> " -> "
                               <> blockName e.callee 0
                               <> " [label = \"" <> e.label <> "\" ]"

                        mbNode = case Map.lookup e.callee mapping of
                            Nothing -> -- add node for a missing item
                                "\n" <> blockName e.callee 0 <> " [ color=red, label = \"" <> e.callee <> "\" ]"
                            Just _ -> ""
                    in Just $ edge <> mbNode
              | otherwise = Nothing


-- compute a short unique name from the item's symbol name and the
-- block index in the body
shortName :: Text -> Text
shortName sym = Text.pack $ "node_" <> showHex (abs $ hash sym) ""

blockName :: Text -> Int -> Text
blockName sym n = Text.pack $ "node_" <> showHex (abs $ hash sym) ("_" <> show n)

brokenAt :: Int -> Text -> Text
brokenAt n t = Text.intercalate "\\l" $ splitN t
  where
    splitN x
        | Text.length x <= n = [x]
        | otherwise = let (hd, rest) = Text.splitAt n x
                      in hd : splitN rest

withBreaks :: Int -> Text -> Text
withBreaks n t
    | Text.length t < n = t
    | otherwise = Text.intercalate "\\n" $ map (brokenAt n) $ Text.words t
