module Data.Smir.Construct (
    construct,
) where

import Data.Map.Strict qualified as Map
import Data.Maybe (catMaybes, fromJust, fromMaybe)
import Data.Text (Text)
import Data.Text qualified as Text

import Data.Smir.Json as Json
import Data.Smir.Cfg as Cfg

construct :: Smir -> Cfg
construct smir = Cfg { name = smir.name, items = Map.fromList $ map processItem smir.items }
  where
    funcMap = Map.map decorateName smir.functions

    decorateName :: Symbol -> Text
    decorateName (NormalSym t) = t
    decorateName (NoOpSym t) = "NoOp: " <> t
    decorateName (IntrinsicSym t) = "Intr: " <> t

    processItem :: Item -> (Text, CfgItem)
    processItem i =
        let (fName, fBody) =
                case i.mono_item_kind of
                    MonoItemFn{name, body = [b]} -> (name, b)
                    -- FIXME this is a hack to deal with more or less than one function body
                    MonoItemFn{name, body = []} -> (name, Body 0 [])
                    MonoItemFn{name, body = bs} ->
                        (name, (head bs){Json.blocks = concatMap (.blocks) bs})
                    other -> error $ "unexpected item kind " <> show other
        in (i.symbol_name, CfgItem { humanName = fName, blocks = map processBlock $ fBody.blocks })

    processBlock :: Block -> CfgBlock
    processBlock b = case b.terminator.kind of
        Json.Drop{target, unwind} ->
            let innerEdges = catMaybes [unwindEdge unwind, fmap (NextEdge Nothing) target]
            in CfgBlock Nothing innerEdges Cfg.Drop
        Json.Assert{target, unwind} ->
            let innerEdges = catMaybes [unwindEdge unwind, fmap (NextEdge Nothing) target]
            in CfgBlock Nothing innerEdges Cfg.Assert
        Json.Call{args, destination, func, target, unwind} ->
            let innerEdges =
                    catMaybes [ unwindEdge unwind
                              , fmap (NextEdge $ Just (showPlace destination)) target
                              ]
            in CfgBlock (Just $ mkCallEdge args func) innerEdges Cfg.Call
        Json.Goto{target} ->
            CfgBlock Nothing [NextEdge Nothing $ fromJust target] Cfg.Assert
        Json.SwitchInt {targets} ->
            let innerEdges =
                    (NextEdge (Just "other") targets.otherwise)
                     : [ NextEdge (Just (Text.pack $ show l)) t| [l, t] <- targets.branches ]
            in CfgBlock Nothing innerEdges Cfg.SwitchInt
        Json.Resume ->
            CfgBlock Nothing [] Cfg.Resume
        Json.Return ->
            CfgBlock Nothing [] Cfg.Return
        Json.Unreachable ->
            CfgBlock Nothing [] Cfg.Unreachable

    unwindEdge :: Unwind -> Maybe NextEdge
    unwindEdge (UCleanup next) = Just $ NextEdge{ next, label = Just "Cleanup"}
    unwindEdge _other = Nothing

    mkCallEdge :: [Operand] -> Operand -> CallEdge
    mkCallEdge args func = CallEdge { callee, label }
        where
          callee =
              case func of
                  Move p -> showPlace p <> " :: Fn"
                  Copy p -> showPlace p <> " :: Fn"
                  Constant{const_ = Const{ty}} ->
                      fromMaybe (Text.pack $ "ty = " <> show ty <> " ??") $
                          Map.lookup ty funcMap

          label = Text.intercalate "," $ map showOp args

          showOp :: Operand -> Text
          showOp (Move p) = showPlace p
          showOp (Copy p) = showPlace p
          showOp Constant{const_ = Const{ty}} = Text.pack $ "const :: <" <> show ty <> ">"

    showPlace :: Place -> Text
    showPlace Place{local, projection} =
        Text.pack $
            "_" <> show local <> if null projection then "" else "(...)"
