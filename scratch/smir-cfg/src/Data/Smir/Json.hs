{-# OPTIONS -Wno-partial-fields #-}
{-# OPTIONS -Wno-incomplete-patterns #-}

module Data.Smir.Json (
  module Data.Smir.Json,
) where

import Data.Aeson as JSON
import Data.Aeson.Types as JSON (Parser)
import Data.Map.Strict (Map)
import Data.Map.Strict qualified as Map
import Data.Text (Text)
import GHC.Generics

-- data model of Stable-MIR json
data Smir = Smir
    { name :: Text
    , crate_id :: Integer
    , allocs :: Map Int Allocation
    , functions :: Map Int Symbol
    -- , uneval_consts :: Something
    , items :: [Item]
    }
    deriving (Eq, Show, Generic)

instance FromJSON Smir where
    parseJSON = withObject "Smir" $ \o -> (Smir
        <$> o .: "name"
        <*> o .: "crate_id"
        <*> (o .: "allocs" >>= toIntMap)
        <*> (o .: "functions" >>= toIntMap)
        <*> o .: "items")

instance ToJSON Smir where
    toJSON smir = object
        [ "name" .= smir.name
        , "crate_id" .= smir.crate_id
        , "allocs" .= Map.assocs smir.allocs
        , "functions" .= Map.assocs smir.functions
        , "items" .= smir.items
        ]

toIntMap :: FromJSON a => [(Int, Value)] -> JSON.Parser (Map Int a)
toIntMap = fmap Map.fromList . mapM parseMapping
    where
        parseMapping (k, x) = (k,) <$> parseJSON x

data Allocation
    = Memory JSON.Value
    | Static Int
    deriving (Eq, Show, Generic)

instance FromJSON Allocation where
    parseJSON = genericParseJSON objectEncoding

instance ToJSON Allocation where
    toJSON = genericToJSON objectEncoding

data Symbol
    = NormalSym Text
    | NoOpSym Text
    | IntrinsicSym Text
    deriving (Eq, Show, Generic)

instance FromJSON Symbol where
    parseJSON = genericParseJSON objectEncoding

instance ToJSON Symbol where
    toJSON = genericToJSON objectEncoding

data Item = Item
    { mono_item_kind :: MonoItemKind
    -- , details :: ()
    , symbol_name :: Text
    }
    deriving (Eq, Show, Generic, FromJSON, ToJSON)

data MonoItemKind
    = MonoItemFn
        { body :: [Body]
        , id :: Int
        , name :: Text
        }
    | Dummy
    deriving (Eq, Show, Generic)

instance FromJSON MonoItemKind where
    parseJSON = genericParseJSON objectEncoding

instance ToJSON MonoItemKind where
    toJSON = genericToJSON objectEncoding

objectEncoding :: JSON.Options
objectEncoding =
    JSON.defaultOptions { sumEncoding = ObjectWithSingleField }

data Body = Body
    { arg_count :: Int
    , blocks :: [ Block ]
    -- , locals :: [Local]
    -- span :: Int
    -- spread_arg :: Something
    -- var_debug_info :: Something
    }
    deriving (Eq, Show, Generic, FromJSON, ToJSON)

data Block = Block
    { terminator :: TerminatorKind -- Terminator
    -- , statements :: [Statement]
    }
    deriving (Eq, Show, Generic, FromJSON, ToJSON)

data TerminatorKind = TerminatorKind { kind :: Terminator, span :: Int }
    deriving (Eq, Show, Generic, FromJSON, ToJSON)

data Terminator
    = Drop
        { place :: Place
        , target :: Maybe Int -- fake maybe
        , unwind :: Unwind
        }
    | Assert
        { cond :: Operand
        , expected :: Bool
        -- , msg :: SomethingSpecial
        , target :: Maybe Int -- fake maybe
        , unwind :: Unwind
        }
    | Call
        { args :: [Operand]
        , destination :: Place
        , func :: Operand
        , target :: Maybe Int
        , unwind :: Unwind
        }
    | Goto
        { target :: Maybe Int -- fake maybe
        }
    | SwitchInt {discr :: Operand, targets :: Targets}
    | Resume
    | Return
    | Unreachable
    deriving (Eq, Show, Generic)

instance FromJSON Terminator where
    parseJSON (String "Resume") = pure Resume
    parseJSON (String "Return") = pure Return
    parseJSON (String "Unreachable") = pure Unreachable
    parseJSON other = genericParseJSON objectEncoding other

instance ToJSON Terminator where
    toJSON Resume = String "Resume"
    toJSON Return = String "Return"
    toJSON Unreachable = String "Unreachable"
    toJSON other = genericToJSON objectEncoding other

data Operand
    = Move Place
    | Copy Place
    | Constant
        { const_ :: Const
        , span :: Int
        }
    deriving (Eq, Show, Generic)

instance FromJSON Operand where
    parseJSON = genericParseJSON objectEncoding

instance ToJSON Operand where
    toJSON = genericToJSON objectEncoding

data Const = Const
    { id :: Int
    -- , kind :: ActualData
    , ty :: Int
    }
    deriving (Eq, Show, Generic, FromJSON, ToJSON)

data Targets = Targets
    { branches :: [[Int]]
    , otherwise :: Int
    }
    deriving (Eq, Show, Generic, FromJSON, ToJSON)

data Place = Place
    { local :: Int
    , projection :: [Projection]
    }
    deriving (Eq, Show, Generic, FromJSON, ToJSON)

data Projection
    = Deref
    | Field [Int]
    deriving (Eq, Show, Generic)

instance FromJSON Projection where
    parseJSON (String "Deref") = pure Deref
    parseJSON (Object o) = Field <$> o .: "Field"

instance ToJSON Projection where
    toJSON Deref = String "Deref"
    toJSON (Field ns) = JSON.object [ "Field" .= ns ]

data Unwind
    = UCleanup Int
    | UContinue
    | UUnreachable
    | UTerminate
    deriving (Eq, Show, Generic)

instance FromJSON Unwind where
    parseJSON (String "Continue") = pure UContinue
    parseJSON (String "Unreachable") = pure UUnreachable
    parseJSON (String "Terminate") = pure UTerminate
    parseJSON (Object o) = do
        n <- o .: "Cleanup"
        pure $ UCleanup n

instance ToJSON Unwind where
    toJSON UContinue = String "Continue"
    toJSON UUnreachable = String "UUnreachable"
    toJSON UTerminate = String "Terminate"
    toJSON (UCleanup n) = JSON.object ["Cleanup" .= n]
