;; T6 zone load plan, one (asset …) per XAsset type.
;; Exported from OpenAssetTools' ZoneCodeGenerator model (ZoneLoad, native word size).
(asset AddonMapEnts
 (array ClipMaterial 12 ClipMaterial)
 (array cbrushside_t 12 cbrushside_t)
 (array cLeafBrushNode_s 20 cLeafBrushNode_s)
 (array cbrush_array_t 96 cbrush_t)
 (array cmodel_t2 76 cmodel_t2)
 (load MapTriggers 24 (full)
  (ifnz 4
   (reuse 4 0
    (alloc 4 (n 4) 0
     (raw 4 1 8 (v MapTriggers (m 0 0) (t 4 u)))
    )
   )
  )
  (ifnz 12
   (reuse 12 0
    (alloc 12 (n 4) 0
     (raw 12 1 32 (v MapTriggers (m 8 0) (t 4 u)))
    )
   )
  )
  (ifnz 20
   (reuse 20 0
    (alloc 20 (n 4) 0
     (raw 20 1 20 (v MapTriggers (m 16 0) (t 4 u)))
    )
   )
  )
 )
 (load ClipInfo 72 (full)
  (ifnz 4
   (reuse 4 0
    (alloc 4 (n 4) 0
     (raw 4 1 20 (v ClipInfo (m 0 0) (t 4 i)))
    )
   )
  )
  (ifnz 12
   (reuse 12 0
    (alloc 12 (n 4) 0
     (arr 12 1 ClipMaterial 1 (v ClipInfo (m 8 0) (t 4 u)))
    )
   )
  )
  (ifnz 20
   (reuse 20 0
    (alloc 20 (n 4) 0
     (arr 20 1 cbrushside_t 1 (v ClipInfo (m 16 0) (t 4 u)))
    )
   )
  )
  (ifnz 28
   (reuse 28 0
    (alloc 28 (n 4) 0
     (arr 28 1 cLeafBrushNode_s 1 (v ClipInfo (m 24 0) (t 4 u)))
    )
   )
  )
  (ifnz 36
   (reuse 36 0
    (alloc 36 (n 2) 0
     (raw 36 1 2 (v ClipInfo (m 32 0) (t 4 u)))
    )
   )
  )
  (ifnz 44
   (alloc 44 (n 4) 0
    (raw 44 1 12 (v ClipInfo (m 40 0) (t 4 u)))
   )
  )
  (ifnz 52
   (alloc 52 (n 2) 0
    (raw 52 1 2 (v ClipInfo (m 48 0) (t 4 u)))
   )
  )
  (ifnz 60
   (reuse 60 0
    (alloc 60 (n 128) 0
     (arr 60 1 cbrush_array_t 1 (v ClipInfo (m 56 0) (t 2 u)))
    )
   )
  )
  (ifnz 64
   (reuse 64 0
    (alloc 64 (n 128) 0
     (raw 64 1 24 (v ClipInfo (m 56 0) (t 2 u)))
    )
   )
  )
  (ifnz 68
   (reuse 68 0
    (alloc 68 (n 4) 0
     (raw 68 1 4 (v ClipInfo (m 56 0) (t 2 u)))
    )
   )
  )
 )
 (load ClipMaterial 12 (full)
  (xstring 0)
 )
 (load cbrushside_t 12 (full)
  (ifnz 0
   (reuse 0 0
    (alloc 0 (n 4) 0
     (raw 0 1 20 (n 1))
    )
   )
  )
 )
 (load cLeafBrushNode_s 20 (full)
  (embedded 8 cLeafBrushNodeData_t 0)
 )
 (load cLeafBrushNodeData_t 12 (full)
  (chain
   (case (op > (v cLeafBrushNode_s (m 2 0) (t 2 i)) (n 0))
    (embedded 0 cLeafBrushNodeLeaf_t 0)
   )
  )
 )
 (load cLeafBrushNodeLeaf_t 4 (full)
  (ifnz 0
   (reuse 0 0
    (alloc 0 (n 2) 0
     (raw 0 1 2 (v cLeafBrushNode_s (m 2 0) (t 2 i)))
    )
   )
  )
 )
 (load cbrush_t 96 (full)
  (ifnz 32
   (reuse 32 0
    (alloc 32 (n 4) 0
     (single 32 cbrushside_t)
    )
   )
  )
  (ifnz 88
   (reuse 88 0
    (alloc 88 (n 4) 0
     (raw 88 1 12 (n 1))
    )
   )
  )
 )
 (load cmodel_t2 76 (full)
  (ifnz 28
   (reuse 28 0
    (alloc 28 (n 4) 0
     (single 28 ClipInfo)
    )
   )
  )
 )
 (load AddonMapEnts 52 (full)
  (push 5)
  (xstring 0)
  (ifnz 4
   (alloc 4 (n 1) 0
    (raw 4 1 1 (v AddonMapEnts (m 8 0) (t 4 i)))
   )
  )
  (embedded 12 MapTriggers 0)
  (ifnz 36
   (reuse 36 0
    (alloc 36 (n 4) 0
     (single 36 ClipInfo)
    )
   )
  )
  (ifnz 44
   (alloc 44 (n 4) 0
    (arr 44 1 cmodel_t2 1 (v AddonMapEnts (m 40 0) (t 4 u)))
   )
  )
  (ifnz 48
   (alloc 48 (n 16) 0
    (raw 48 1 64 (v AddonMapEnts (m 40 0) (t 4 u)))
   )
  )
  (pop)
 )
 (loadptr AddonMapEnts 1 (n 4))
)
(asset clipMap_t
 (array ClipMaterial 12 ClipMaterial)
 (array cbrushside_t 12 cbrushside_t)
 (array cLeafBrushNode_s 20 cLeafBrushNode_s)
 (array cbrush_array_t 96 cbrush_t)
 (array cStaticModel_s 84 cStaticModel_s)
 (array cNode_t 8 cNode_t)
 (array cmodel_t 76 cmodel_t)
 (array DynEntityDef 84 DynEntityDef)
 (array XModelPiece 16 XModelPiece)
 (array PhysConstraint 168 PhysConstraint)
 (load ClipInfo 72 (full)
  (ifnz 4
   (reuse 4 0
    (alloc 4 (n 4) 0
     (raw 4 1 20 (v ClipInfo (m 0 0) (t 4 i)))
    )
   )
  )
  (ifnz 12
   (reuse 12 0
    (alloc 12 (n 4) 0
     (arr 12 1 ClipMaterial 1 (v ClipInfo (m 8 0) (t 4 u)))
    )
   )
  )
  (ifnz 20
   (reuse 20 0
    (alloc 20 (n 4) 0
     (arr 20 1 cbrushside_t 1 (v ClipInfo (m 16 0) (t 4 u)))
    )
   )
  )
  (ifnz 28
   (reuse 28 0
    (alloc 28 (n 4) 0
     (arr 28 1 cLeafBrushNode_s 1 (v ClipInfo (m 24 0) (t 4 u)))
    )
   )
  )
  (ifnz 36
   (reuse 36 0
    (alloc 36 (n 2) 0
     (raw 36 1 2 (v ClipInfo (m 32 0) (t 4 u)))
    )
   )
  )
  (ifnz 44
   (alloc 44 (n 4) 0
    (raw 44 1 12 (v ClipInfo (m 40 0) (t 4 u)))
   )
  )
  (ifnz 52
   (alloc 52 (n 2) 0
    (raw 52 1 2 (v ClipInfo (m 48 0) (t 4 u)))
   )
  )
  (ifnz 60
   (reuse 60 0
    (alloc 60 (n 128) 0
     (arr 60 1 cbrush_array_t 1 (v ClipInfo (m 56 0) (t 2 u)))
    )
   )
  )
  (ifnz 64
   (reuse 64 0
    (alloc 64 (n 128) 0
     (raw 64 1 24 (v ClipInfo (m 56 0) (t 2 u)))
    )
   )
  )
  (ifnz 68
   (reuse 68 0
    (alloc 68 (n 4) 0
     (raw 68 1 4 (v ClipInfo (m 56 0) (t 2 u)))
    )
   )
  )
 )
 (load ClipMaterial 12 (full)
  (xstring 0)
 )
 (load cbrushside_t 12 (full)
  (ifnz 0
   (reuse 0 0
    (alloc 0 (n 4) 0
     (raw 0 1 20 (n 1))
    )
   )
  )
 )
 (load cLeafBrushNode_s 20 (full)
  (embedded 8 cLeafBrushNodeData_t 0)
 )
 (load cLeafBrushNodeData_t 12 (full)
  (chain
   (case (op > (v cLeafBrushNode_s (m 2 0) (t 2 i)) (n 0))
    (embedded 0 cLeafBrushNodeLeaf_t 0)
   )
  )
 )
 (load cLeafBrushNodeLeaf_t 4 (full)
  (ifnz 0
   (reuse 0 0
    (alloc 0 (n 2) 0
     (raw 0 1 2 (v cLeafBrushNode_s (m 2 0) (t 2 i)))
    )
   )
  )
 )
 (load cbrush_t 96 (full)
  (ifnz 32
   (reuse 32 0
    (alloc 32 (n 4) 0
     (single 32 cbrushside_t)
    )
   )
  )
  (ifnz 88
   (reuse 88 0
    (alloc 88 (n 4) 0
     (raw 88 1 12 (n 1))
    )
   )
  )
 )
 (load cStaticModel_s 84 (full)
  (ifnz 4
   (assetload 4 XModel)
  )
 )
 (load cNode_t 8 (full)
  (ifnz 0
   (reuse 0 0
    (alloc 0 (n 4) 0
     (raw 0 1 20 (n 1))
    )
   )
  )
 )
 (load cmodel_t 76 (full)
  (block 0
   (ifnz 28
    (reuse 28 1
     (alloc 28 (n 4) 1
      (single 28 ClipInfo)
     )
    )
   )
  )
 )
 (load DynEntityDef 84 (full)
  (ifnz 32
   (assetload 32 XModel)
  )
  (ifnz 36
   (assetload 36 XModel)
  )
  (ifnz 44
   (assetload 44 FxEffectDef)
  )
  (ifnz 52
   (reuse 52 0
    (alloc 52 (n 4) 0
     (single 52 XModelPieces)
    )
   )
  )
  (ifnz 56
   (assetload 56 PhysPreset)
  )
 )
 (load XModelPieces 12 (full)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 4) 0
    (arr 8 1 XModelPiece 1 (v XModelPieces (m 4 0) (t 4 i)))
   )
  )
 )
 (load XModelPiece 16 (full)
  (ifnz 0
   (assetload 0 XModel)
  )
 )
 (load PhysConstraint 168 (full)
  (xstring 20)
  (xstring 36)
  (ifnz 140
   (assetload 140 Material)
  )
 )
 (load clipMap_t 332 (full)
  (push 5)
  (xstring 0)
  (embedded 8 ClipInfo 0)
  (block 0
   (ifnz 80
    (reuse 80 1
     (alloc 80 (n 4) 1
      (single 80 ClipInfo)
     )
    )
   )
  )
  (ifnz 88
   (alloc 88 (n 4) 0
    (arr 88 1 cStaticModel_s 1 (v clipMap_t (m 84 0) (t 4 u)))
   )
  )
  (ifnz 96
   (alloc 96 (n 4) 0
    (arr 96 1 cNode_t 1 (v clipMap_t (m 92 0) (t 4 u)))
   )
  )
  (ifnz 104
   (alloc 104 (n 4) 0
    (raw 104 1 44 (v clipMap_t (m 100 0) (t 4 u)))
   )
  )
  (ifnz 112
   (alloc 112 (n 4) 0
    (raw 112 1 12 (v clipMap_t (m 108 0) (t 4 u)))
   )
  )
  (ifnz 120
   (alloc 120 (n 2) 0
    (raw 120 1 6 (v clipMap_t (m 116 0) (t 4 i)))
   )
  )
  (ifnz 124
   (alloc 124 (n 1) 0
    (raw 124 1 1 (op * (op / (op + (op * (n 3) (v clipMap_t (m 116 0) (t 4 i))) (n 31)) (n 32)) (n 4)))
   )
  )
  (ifnz 132
   (alloc 132 (n 4) 0
    (raw 132 1 16 (v clipMap_t (m 128 0) (t 4 i)))
   )
  )
  (ifnz 140
   (alloc 140 (n 16) 0
    (raw 140 1 32 (v clipMap_t (m 136 0) (t 4 i)))
   )
  )
  (ifnz 148
   (alloc 148 (n 4) 0
    (arr 148 1 cmodel_t 1 (v clipMap_t (m 144 0) (t 4 u)))
   )
  )
  (ifnz 160
   (alloc 160 (n 1) 0
    (raw 160 1 1 (op * (v clipMap_t (m 152 0) (t 4 i)) (v clipMap_t (m 156 0) (t 4 i))))
   )
  )
  (ifnz 168
   (assetload 168 MapEnts)
  )
  (ifnz 172
   (reuse 172 0
    (alloc 172 (n 16) 0
     (single 172 cbrush_t)
    )
   )
  )
  (embedded 176 cmodel_t 0)
  (ifnz 264
   (alloc 264 (n 4) 0
    (arr 264 1 DynEntityDef 1 (v clipMap_t (m 254 0) (i (n 0) 2) (t 2 u)))
   )
  )
  (ifnz 268
   (alloc 268 (n 4) 0
    (arr 268 1 DynEntityDef 1 (v clipMap_t (m 254 0) (i (n 1) 2) (t 2 u)))
   )
  )
  (block 1
   (ifnz 272
    (alloc 272 (n 4) 0
     (raw 272 1 32 (v clipMap_t (m 254 0) (i (n 0) 2) (t 2 u)))
    )
   )
  )
  (block 1
   (ifnz 276
    (alloc 276 (n 4) 0
     (raw 276 1 32 (v clipMap_t (m 254 0) (i (n 1) 2) (t 2 u)))
    )
   )
  )
  (block 1
   (ifnz 280
    (alloc 280 (n 4) 0
     (raw 280 1 20 (v clipMap_t (m 254 0) (i (n 0) 2) (t 2 u)))
    )
   )
  )
  (block 1
   (ifnz 284
    (alloc 284 (n 4) 0
     (raw 284 1 20 (v clipMap_t (m 254 0) (i (n 1) 2) (t 2 u)))
    )
   )
  )
  (block 1
   (ifnz 288
    (alloc 288 (n 4) 0
     (raw 288 1 8 (v clipMap_t (m 254 0) (i (n 2) 2) (t 2 u)))
    )
   )
  )
  (block 1
   (ifnz 292
    (alloc 292 (n 4) 0
     (raw 292 1 8 (v clipMap_t (m 254 0) (i (n 3) 2) (t 2 u)))
    )
   )
  )
  (block 1
   (ifnz 296
    (alloc 296 (n 4) 0
     (raw 296 1 32 (v clipMap_t (m 254 0) (i (n 0) 2) (t 2 u)))
    )
   )
  )
  (block 1
   (ifnz 300
    (alloc 300 (n 4) 0
     (raw 300 1 32 (v clipMap_t (m 254 0) (i (n 1) 2) (t 2 u)))
    )
   )
  )
  (block 1
   (ifnz 304
    (alloc 304 (n 4) 0
     (raw 304 1 32 (v clipMap_t (m 254 0) (i (n 2) 2) (t 2 u)))
    )
   )
  )
  (block 1
   (ifnz 308
    (alloc 308 (n 4) 0
     (raw 308 1 32 (v clipMap_t (m 254 0) (i (n 3) 2) (t 2 u)))
    )
   )
  )
  (ifnz 316
   (alloc 316 (n 4) 0
    (arr 316 1 PhysConstraint 1 (v clipMap_t (m 312 0) (t 4 i)))
   )
  )
  (block 1
   (ifnz 324
    (alloc 324 (n 4) 0
     (raw 324 1 3188 (v clipMap_t (m 320 0) (t 4 i)))
    )
   )
  )
  (pop)
 )
 (loadptr clipMap_t 1 (n 4))
)
(asset ComWorld
 (array ComPrimaryLight 196 ComPrimaryLight)
 (load ComPrimaryLight 196 (full)
  (xstring 192)
 )
 (load ComWorld 16 (full)
  (push 5)
  (xstring 0)
  (ifnz 12
   (alloc 12 (n 4) 0
    (arr 12 1 ComPrimaryLight 1 (v ComWorld (m 8 0) (t 4 u)))
   )
  )
  (pop)
 )
 (loadptr ComWorld 1 (n 4))
)
(asset ddlRoot_t
 (array ddlStructDef_t 20 ddlStructDef_t)
 (array ddlMemberDef_t 44 ddlMemberDef_t)
 (array ddlEnumDef_t 16 ddlEnumDef_t)
 (load ddlDef_t 28 (full)
  (ifnz 8
   (alloc 8 (n 4) 0
    (arr 8 1 ddlStructDef_t 1 (v ddlDef_t (m 12 0) (t 4 i)))
   )
  )
  (ifnz 16
   (alloc 16 (n 4) 0
    (arr 16 1 ddlEnumDef_t 1 (v ddlDef_t (m 20 0) (t 4 i)))
   )
  )
  (ifnz 24
   (alloc 24 (n 4) 0
    (single 24 ddlDef_t)
   )
  )
 )
 (load ddlStructDef_t 20 (full)
  (xstring 0)
  (ifnz 12
   (alloc 12 (n 4) 0
    (arr 12 1 ddlMemberDef_t 1 (v ddlStructDef_t (m 8 0) (t 4 i)))
   )
  )
  (ifnz 16
   (alloc 16 (n 4) 0
    (raw 16 1 8 (v ddlStructDef_t (m 8 0) (t 4 i)))
   )
  )
 )
 (load ddlMemberDef_t 44 (full)
  (xstring 0)
 )
 (load ddlEnumDef_t 16 (full)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 4) 0
    (xstrarr 8 1 1 (v ddlEnumDef_t (m 4 0) (t 4 i)))
   )
  )
  (ifnz 12
   (alloc 12 (n 4) 0
    (raw 12 1 8 (v ddlEnumDef_t (m 4 0) (t 4 i)))
   )
  )
 )
 (load ddlRoot_t 8 (full)
  (push 5)
  (xstring 0)
  (ifnz 4
   (alloc 4 (n 4) 0
    (single 4 ddlDef_t)
   )
  )
  (pop)
 )
 (loadptr ddlRoot_t 1 (n 4))
)
(asset DestructibleDef
 (ptrarray XModel 248 XModel 0 1 1 (n 4))
 (array DestructiblePiece 312 DestructiblePiece)
 (array DestructibleStage 48 DestructibleStage)
 (load DestructiblePiece 312 (full)
  (arr 0 0 DestructibleStage 0 (n 5))
  (ifnz 268
   (assetload 268 PhysConstraints)
  )
  (xstring 276)
  (ifnz 280
   (assetload 280 FxEffectDef)
  )
  (xstring 284)
 )
 (load DestructibleStage 48 (full)
  (ifnz 16
   (assetload 16 FxEffectDef)
  )
  (xstring 20)
  (xstring 24)
  (xstring 28)
  (ptrarr 32 0 XModel 0 (n 3))
  (ifnz 44
   (assetload 44 PhysPreset)
  )
 )
 (load DestructibleDef 24 (full)
  (push 5)
  (xstring 0)
  (ifnz 4
   (assetload 4 XModel)
  )
  (ifnz 8
   (assetload 8 XModel)
  )
  (ifnz 16
   (alloc 16 (n 4) 0
    (arr 16 1 DestructiblePiece 1 (v DestructibleDef (m 12 0) (t 4 i)))
   )
  )
  (pop)
 )
 (loadptr DestructibleDef 1 (n 4))
)
(asset EmblemSet
 (array EmblemCategory 8 EmblemCategory)
 (array EmblemIconType 8 EmblemIconType)
 (array EmblemBGCategory 8 EmblemBGCategory)
 (array EmblemIcon 36 EmblemIcon)
 (array EmblemBackground 36 EmblemBackground)
 (load EmblemCategory 8 (full)
  (xstring 0)
  (xstring 4)
 )
 (load EmblemIconType 8 (full)
  (xstring 0)
  (xstring 4)
 )
 (load EmblemBGCategory 8 (full)
  (xstring 0)
  (xstring 4)
 )
 (load EmblemIcon 36 (full)
  (ifnz 0
   (assetload 0 GfxImage)
  )
  (xstring 4)
 )
 (load EmblemBackground 36 (full)
  (ifnz 0
   (assetload 0 Material)
  )
  (xstring 4)
 )
 (load EmblemSet 60 (full)
  (push 5)
  (ifnz 8
   (alloc 8 (n 4) 0
    (raw 8 1 12 (v EmblemSet (m 4 0) (t 4 i)))
   )
  )
  (ifnz 16
   (alloc 16 (n 4) 0
    (arr 16 1 EmblemCategory 1 (v EmblemSet (m 12 0) (t 4 i)))
   )
  )
  (ifnz 24
   (alloc 24 (n 4) 0
    (arr 24 1 EmblemIconType 1 (v EmblemSet (m 20 0) (t 4 i)))
   )
  )
  (ifnz 32
   (alloc 32 (n 4) 0
    (arr 32 1 EmblemBGCategory 1 (v EmblemSet (m 20 0) (t 4 i)))
   )
  )
  (ifnz 40
   (alloc 40 (n 4) 0
    (arr 40 1 EmblemIcon 1 (v EmblemSet (m 20 0) (t 4 i)))
   )
  )
  (ifnz 48
   (alloc 48 (n 4) 0
    (arr 48 1 EmblemBackground 1 (v EmblemSet (m 44 0) (t 4 i)))
   )
  )
  (ifnz 56
   (alloc 56 (n 2) 0
    (raw 56 1 2 (v EmblemSet (m 52 0) (t 4 i)))
   )
  )
  (pop)
 )
 (loadptr EmblemSet 1 (n 4))
)
(asset FontIcon
 (array FontIconEntry 24 FontIconEntry)
 (load FontIconEntry 24 (full)
  (embedded 0 FontIconName 0)
  (ifnz 8
   (assetload 8 Material)
  )
 )
 (load FontIconName 8 (full)
  (xstring 0)
 )
 (load FontIcon 20 (full)
  (push 5)
  (xstring 0)
  (ifnz 12
   (reuse 12 0
    (alloc 12 (n 4) 0
     (arr 12 1 FontIconEntry 1 (v FontIcon (m 4 0) (t 4 u)))
    )
   )
  )
  (ifnz 16
   (reuse 16 0
    (alloc 16 (n 4) 0
     (raw 16 1 8 (v FontIcon (m 8 0) (t 4 u)))
    )
   )
  )
  (pop)
 )
 (loadptr FontIcon 1 (n 4))
)
(asset Font_s
 (load Font_s 36 (full)
  (push 5)
  (xstring 0)
  (ifnz 20
   (assetload 20 Material)
  )
  (ifnz 24
   (assetload 24 Material)
  )
  (ifnz 28
   (reuse 28 0
    (alloc 28 (n 4) 0
     (raw 28 1 24 (v Font_s (m 12 0) (t 4 i)))
    )
   )
  )
  (ifnz 32
   (reuse 32 0
    (alloc 32 (n 4) 0
     (raw 32 1 8 (v Font_s (m 16 0) (t 4 i)))
    )
   )
  )
  (pop)
 )
 (loadptr Font_s 1 (n 4))
)
(asset FootstepFXTableDef
 (ptrarray FxEffectDef 76 FxEffectDef 0 1 1 (n 4))
 (load FootstepFXTableDef 132 (full)
  (push 5)
  (xstring 0)
  (ptrarr 4 0 FxEffectDef 0 (n 32))
  (pop)
 )
 (loadptr FootstepFXTableDef 1 (n 4))
)
(asset FootstepTableDef
 (load FootstepTableDef 900 (full)
  (push 5)
  (xstring 0)
  (pop)
 )
 (loadptr FootstepTableDef 1 (n 4))
)
(asset FxEffectDef
 (ptrarray Material 112 Material 0 1 1 (n 4))
 (array FxElemDef 292 FxElemDef)
 (array FxElemMarkVisuals 8 FxElemMarkVisuals)
 (array FxElemVisuals 4 FxElemVisuals)
 (load FxElemDef 292 (full)
  (ifnz 188
   (alloc 188 (n 4) 0
    (raw 188 1 96 (op + (v FxElemDef (m 186 0) (t 1 i)) (n 1)))
   )
  )
  (ifnz 192
   (alloc 192 (n 4) 0
    (raw 192 1 48 (op + (v FxElemDef (m 187 0) (t 1 i)) (n 1)))
   )
  )
  (embedded 196 FxElemDefVisuals 0)
  (embedded 224 FxEffectDefRef 0)
  (embedded 228 FxEffectDefRef 0)
  (embedded 232 FxEffectDefRef 0)
  (embedded 252 FxEffectDefRef 0)
  (embedded 256 FxElemExtendedDefPtr 0)
  (embedded 280 FxElemSpawnSound 0)
 )
 (load FxElemDefVisuals 4 (full)
  (chain
   (case (op == (v FxElemDef (m 184 0) (t 1 i)) (n 11))
    (ifnz 0
     (alloc 0 (n 4) 0
      (arr 0 1 FxElemMarkVisuals 1 (v FxElemDef (m 185 0) (t 1 i)))
     )
    )
   )
   (case (op > (v FxElemDef (m 185 0) (t 1 i)) (n 1))
    (ifnz 0
     (alloc 0 (n 4) 0
      (arr 0 1 FxElemVisuals 1 (v FxElemDef (m 185 0) (t 1 i)))
     )
    )
   )
   (case (else)
    (embedded 0 FxElemVisuals 0)
   )
  )
 )
 (load FxElemMarkVisuals 8 (full)
  (ptrarr 0 0 Material 0 (n 2))
 )
 (load FxElemVisuals 4 (full)
  (chain
   (case (op || (op || (op || (op || (op || (op || (op == (v FxElemDef (m 184 0) (t 1 i)) (n 0)) (op == (v FxElemDef (m 184 0) (t 1 i)) (n 1))) (op == (v FxElemDef (m 184 0) (t 1 i)) (n 2))) (op == (v FxElemDef (m 184 0) (t 1 i)) (n 3))) (op == (v FxElemDef (m 184 0) (t 1 i)) (n 4))) (op == (v FxElemDef (m 184 0) (t 1 i)) (n 5))) (op == (v FxElemDef (m 184 0) (t 1 i)) (n 6)))
    (ifnz 0
     (assetload 0 Material)
    )
   )
   (case (op == (v FxElemDef (m 184 0) (t 1 i)) (n 7))
    (ifnz 0
     (assetload 0 XModel)
    )
   )
   (case (op == (v FxElemDef (m 184 0) (t 1 i)) (n 12))
    (embedded 0 FxEffectDefRef 0)
   )
   (case (op == (v FxElemDef (m 184 0) (t 1 i)) (n 10))
    (xstring 0)
   )
   (case (op == (v FxElemDef (m 184 0) (t 1 i)) (n 9))
    (ifnz 0
     (assetload 0 GfxLightDef)
    )
   )
  )
 )
 (load FxEffectDefRef 4 (full)
  (chain
   (case (else)
    (xstring 0)
   )
  )
 )
 (load FxElemExtendedDefPtr 4 (full)
  (chain
   (case (op == (v FxElemDef (m 184 0) (t 1 i)) (n 5))
    (ifnz 0
     (alloc 0 (n 4) 0
      (single 0 FxTrailDef)
     )
    )
   )
   (case (op == (v FxElemDef (m 184 0) (t 1 i)) (n 9))
    (ifnz 0
     (alloc 0 (n 4) 0
      (raw 0 1 12 (n 1))
     )
    )
   )
   (case (else)
    (ifnz 0
     (alloc 0 (n 1) 0
      (raw 0 1 1 (n 1))
     )
    )
   )
  )
 )
 (load FxTrailDef 28 (full)
  (ifnz 16
   (alloc 16 (n 4) 0
    (raw 16 1 20 (v FxTrailDef (m 12 0) (t 4 i)))
   )
  )
  (ifnz 24
   (alloc 24 (n 2) 0
    (raw 24 1 2 (v FxTrailDef (m 20 0) (t 4 i)))
   )
  )
 )
 (load FxElemSpawnSound 4 (full)
  (xstring 0)
 )
 (load FxEffectDef 76 (full)
  (push 5)
  (xstring 0)
  (ifnz 28
   (alloc 28 (n 4) 0
    (arr 28 1 FxElemDef 1 (op + (op + (v FxEffectDef (m 8 0) (t 2 i)) (v FxEffectDef (m 10 0) (t 2 i))) (v FxEffectDef (m 12 0) (t 2 i))))
   )
  )
  (pop)
 )
 (loadptr FxEffectDef 1 (n 4))
)
(asset FxImpactTable
 (ptrarray FxEffectDef 76 FxEffectDef 0 1 1 (n 4))
 (array FxImpactEntry 144 FxImpactEntry)
 (load FxImpactEntry 144 (full)
  (ptrarr 0 0 FxEffectDef 0 (n 32))
  (ptrarr 128 0 FxEffectDef 0 (n 4))
 )
 (load FxImpactTable 8 (full)
  (push 5)
  (xstring 0)
  (ifnz 4
   (alloc 4 (n 4) 0
    (arr 4 1 FxImpactEntry 1 (n 21))
   )
  )
  (pop)
 )
 (loadptr FxImpactTable 1 (n 4))
)
(asset GameWorldMp
 (ptrarray pathnode_tree_t 16 pathnode_tree_t 1 0 1 (n 4))
 (array pathnode_t 144 pathnode_t)
 (array pathnode_tree_t 16 pathnode_tree_t)
 (load PathData 40 (full)
  (ifnz 8
   (alloc 8 (n 4) 0
    (arr 8 1 pathnode_t 1 (op + (v PathData (m 0 0) (t 4 u)) (n 128)))
   )
  )
  (block 1
   (ifnz 12
    (alloc 12 (n 16) 0
     (raw 12 1 16 (op + (v PathData (m 0 0) (t 4 u)) (n 128)))
    )
   )
  )
  (ifnz 20
   (alloc 20 (n 1) 0
    (raw 20 1 1 (v PathData (m 16 0) (t 4 i)))
   )
  )
  (ifnz 28
   (alloc 28 (n 1) 0
    (raw 28 1 1 (v PathData (m 24 0) (t 4 i)))
   )
  )
  (ifnz 36
   (alloc 36 (n 4) 0
    (arr 36 1 pathnode_tree_t 1 (v PathData (m 32 0) (t 4 i)))
   )
  )
 )
 (load pathnode_t 144 (full)
  (embedded 0 pathnode_constant_t 0)
 )
 (load pathnode_constant_t 68 (full)
  (ifnz 64
   (alloc 64 (n 4) 0
    (raw 64 1 16 (v pathnode_constant_t (m 60 0) (t 2 u)))
   )
  )
 )
 (load pathnode_tree_t 16 (full)
  (embedded 8 pathnode_tree_info_t 0)
 )
 (load pathnode_tree_info_t 8 (full)
  (chain
   (case (op < (v pathnode_tree_t (m 0 0) (t 4 i)) (n 0))
    (embedded 0 pathnode_tree_nodes_t 0)
   )
   (case (else)
    (ptrarr 0 0 pathnode_tree_t 0 (n 2))
   )
  )
 )
 (load pathnode_tree_nodes_t 8 (full)
  (ifnz 4
   (alloc 4 (n 2) 0
    (raw 4 1 2 (v pathnode_tree_nodes_t (m 0 0) (t 4 i)))
   )
  )
 )
 (load GameWorldMp 44 (full)
  (push 5)
  (xstring 0)
  (embedded 4 PathData 0)
  (pop)
 )
 (loadptr GameWorldMp 1 (n 4))
)
(asset GameWorldSp
 (ptrarray pathnode_tree_t 16 pathnode_tree_t 1 0 1 (n 4))
 (array pathnode_t 144 pathnode_t)
 (array pathnode_tree_t 16 pathnode_tree_t)
 (load PathData 40 (full)
  (ifnz 8
   (alloc 8 (n 4) 0
    (arr 8 1 pathnode_t 1 (op + (v PathData (m 0 0) (t 4 u)) (n 128)))
   )
  )
  (block 1
   (ifnz 12
    (alloc 12 (n 16) 0
     (raw 12 1 16 (op + (v PathData (m 0 0) (t 4 u)) (n 128)))
    )
   )
  )
  (ifnz 20
   (alloc 20 (n 1) 0
    (raw 20 1 1 (v PathData (m 16 0) (t 4 i)))
   )
  )
  (ifnz 28
   (alloc 28 (n 1) 0
    (raw 28 1 1 (v PathData (m 24 0) (t 4 i)))
   )
  )
  (ifnz 36
   (alloc 36 (n 4) 0
    (arr 36 1 pathnode_tree_t 1 (v PathData (m 32 0) (t 4 i)))
   )
  )
 )
 (load pathnode_t 144 (full)
  (embedded 0 pathnode_constant_t 0)
 )
 (load pathnode_constant_t 68 (full)
  (ifnz 64
   (alloc 64 (n 4) 0
    (raw 64 1 16 (v pathnode_constant_t (m 60 0) (t 2 u)))
   )
  )
 )
 (load pathnode_tree_t 16 (full)
  (embedded 8 pathnode_tree_info_t 0)
 )
 (load pathnode_tree_info_t 8 (full)
  (chain
   (case (op < (v pathnode_tree_t (m 0 0) (t 4 i)) (n 0))
    (embedded 0 pathnode_tree_nodes_t 0)
   )
   (case (else)
    (ptrarr 0 0 pathnode_tree_t 0 (n 2))
   )
  )
 )
 (load pathnode_tree_nodes_t 8 (full)
  (ifnz 4
   (alloc 4 (n 2) 0
    (raw 4 1 2 (v pathnode_tree_nodes_t (m 0 0) (t 4 i)))
   )
  )
 )
 (load GameWorldSp 44 (full)
  (push 5)
  (xstring 0)
  (embedded 4 PathData 0)
  (pop)
 )
 (loadptr GameWorldSp 1 (n 4))
)
(asset GfxImage
 (load GfxTexture 4 (full)
  (chain
   (case (else)
    (block 0
     (ifnz 0
      (reuse 0 1
       (alloc 0 (n 4) 1
        (single 0 GfxImageLoadDef)
       )
      )
     )
    )
   )
  )
 )
 (load GfxImageLoadDef 16 (partial 12)
  (raw 12 0 1 (v GfxImageLoadDef (m 8 0) (t 4 i)))
 )
 (load GfxImage 80 (full)
  (push 5)
  (xstring 72)
  (embedded 0 GfxTexture 0)
  (pop)
 )
 (loadptr GfxImage 1 (n 4))
)
(asset GfxLightDef
 (load GfxLightImage 8 (full)
  (ifnz 0
   (assetload 0 GfxImage)
  )
 )
 (load GfxLightDef 16 (full)
  (push 5)
  (xstring 0)
  (embedded 4 GfxLightImage 0)
  (pop)
 )
 (loadptr GfxLightDef 1 (n 4))
)
(asset GfxWorld
 (array GfxCell 48 GfxCell)
 (array GfxAabbTree 40 GfxAabbTree)
 (array GfxPortal 92 GfxPortal)
 (array GfxReflectionProbe 76 GfxReflectionProbe)
 (array GfxLightmapArray 8 GfxLightmapArray)
 (array MaterialMemory 8 MaterialMemory)
 (array SSkinInstance 96 SSkinInstance)
 (array GfxShadowGeometry 12 GfxShadowGeometry)
 (array GfxLightRegion 8 GfxLightRegion)
 (array GfxLightRegionHull 80 GfxLightRegionHull)
 (array GfxSurface 80 GfxSurface)
 (array GfxStaticModelDrawInst 152 GfxStaticModelDrawInst)
 (array GfxStaticModelLmapVertexInfo 12 GfxStaticModelLmapVertexInfo)
 (array GfxWaterBuffer 8 GfxWaterBuffer)
 (load GfxWorldStreamInfo 16 (full)
  (ifnz 4
   (alloc 4 (n 16) 0
    (raw 4 1 48 (v GfxWorldStreamInfo (m 0 0) (t 4 i)))
   )
  )
  (ifnz 12
   (alloc 12 (n 4) 0
    (raw 12 1 4 (v GfxWorldStreamInfo (m 8 0) (t 4 i)))
   )
  )
 )
 (load GfxLight 352 (full)
  (ifnz 336
   (assetload 336 GfxLightDef)
  )
 )
 (load GfxWorldDpvsPlanes 16 (full)
  (ifnz 4
   (reuse 4 0
    (alloc 4 (n 4) 0
     (raw 4 1 20 (v GfxWorld (m 8 0) (t 4 i)))
    )
   )
  )
  (ifnz 8
   (alloc 8 (n 2) 0
    (raw 8 1 2 (v GfxWorld (m 12 0) (t 4 i)))
   )
  )
  (block 1
   (ifnz 12
    (alloc 12 (n 4) 0
     (raw 12 1 4 (op * (v GfxWorldDpvsPlanes (m 0 0) (t 4 i)) (n 512)))
    )
   )
  )
 )
 (load GfxCell 48 (full)
  (ifnz 28
   (alloc 28 (n 4) 0
    (arr 28 1 GfxAabbTree 1 (v GfxCell (m 24 0) (t 4 i)))
   )
  )
  (ifnz 36
   (alloc 36 (n 4) 0
    (arr 36 1 GfxPortal 1 (v GfxCell (m 32 0) (t 4 i)))
   )
  )
  (ifnz 44
   (alloc 44 (n 1) 0
    (raw 44 1 1 (v GfxCell (m 40 0) (t 1 i)))
   )
  )
 )
 (load GfxAabbTree 40 (full)
  (ifnz 32
   (reuse 32 0
    (alloc 32 (n 2) 0
     (raw 32 1 2 (v GfxAabbTree (m 30 0) (t 2 u)))
    )
   )
  )
 )
 (load GfxPortal 92 (full)
  (ifnz 32
   (reuse 32 0
    (alloc 32 (n 4) 0
     (single 32 GfxCell)
    )
   )
  )
  (ifnz 36
   (alloc 36 (n 4) 0
    (raw 36 1 12 (v GfxPortal (m 40 0) (t 1 i)))
   )
  )
 )
 (load GfxWorldDraw 68 (full)
  (ifnz 4
   (alloc 4 (n 4) 0
    (arr 4 1 GfxReflectionProbe 1 (v GfxWorldDraw (m 0 0) (t 4 u)))
   )
  )
  (block 1
   (ifnz 8
    (alloc 8 (n 4) 0
     (raw 8 1 4 (v GfxWorldDraw (m 0 0) (t 4 u)))
    )
   )
  )
  (ifnz 16
   (alloc 16 (n 4) 0
    (arr 16 1 GfxLightmapArray 1 (v GfxWorldDraw (m 12 0) (t 4 i)))
   )
  )
  (block 1
   (ifnz 20
    (alloc 20 (n 4) 0
     (raw 20 1 4 (v GfxWorldDraw (m 12 0) (t 4 i)))
    )
   )
  )
  (block 1
   (ifnz 24
    (alloc 24 (n 4) 0
     (raw 24 1 4 (v GfxWorldDraw (m 12 0) (t 4 i)))
    )
   )
  )
  (embedded 36 GfxWorldVertexData0 0)
  (embedded 48 GfxWorldVertexData1 0)
  (ifnz 60
   (alloc 60 (n 2) 0
    (raw 60 1 2 (v GfxWorldDraw (m 56 0) (t 4 i)))
   )
  )
 )
 (load GfxReflectionProbe 76 (full)
  (ifnz 60
   (assetload 60 GfxImage)
  )
  (ifnz 64
   (alloc 64 (n 4) 0
    (raw 64 1 96 (v GfxReflectionProbe (m 68 0) (t 4 u)))
   )
  )
 )
 (load GfxLightmapArray 8 (full)
  (ifnz 0
   (assetload 0 GfxImage)
  )
  (ifnz 4
   (assetload 4 GfxImage)
  )
 )
 (load GfxWorldVertexData0 8 (full)
  (ifnz 0
   (alloc 0 (n 128) 0
    (raw 0 1 1 (v GfxWorldDraw (m 32 0) (t 4 u)))
   )
  )
 )
 (load GfxWorldVertexData1 8 (full)
  (ifnz 0
   (alloc 0 (n 128) 0
    (raw 0 1 1 (v GfxWorldDraw (m 44 0) (t 4 u)))
   )
  )
 )
 (load GfxLightGrid 72 (full)
  (ifnz 28
   (alloc 28 (n 2) 0
    (raw 28 1 2 (op + (op - (v GfxLightGrid (m 10 0) (i (v GfxLightGrid (m 20 0) (t 4 u)) 2) (t 2 u)) (v GfxLightGrid (m 4 0) (i (v GfxLightGrid (m 20 0) (t 4 u)) 2) (t 2 u))) (n 1)))
   )
  )
  (ifnz 36
   (alloc 36 (n 4) 0
    (raw 36 1 1 (v GfxLightGrid (m 32 0) (t 4 u)))
   )
  )
  (ifnz 44
   (alloc 44 (n 4) 0
    (raw 44 1 4 (v GfxLightGrid (m 40 0) (t 4 u)))
   )
  )
  (ifnz 52
   (alloc 52 (n 4) 0
    (raw 52 1 168 (v GfxLightGrid (m 48 0) (t 4 u)))
   )
  )
  (ifnz 60
   (alloc 60 (n 4) 0
    (raw 60 1 54 (v GfxLightGrid (m 56 0) (t 4 u)))
   )
  )
  (ifnz 68
   (alloc 68 (n 4) 0
    (raw 68 1 40 (v GfxLightGrid (m 64 0) (t 4 u)))
   )
  )
 )
 (load MaterialMemory 8 (full)
  (ifnz 0
   (assetload 0 Material)
  )
 )
 (load sunflare_t 96 (full)
  (ifnz 4
   (assetload 4 Material)
  )
  (ifnz 8
   (assetload 8 Material)
  )
 )
 (load SSkinInstance 96 (full)
  (ifnz 64
   (reuse 64 0
    (alloc 64 (n 4) 0
     (single 64 SSkinShaders)
    )
   )
  )
  (ifnz 68
   (reuse 68 0
    (alloc 68 (n 4) 0
     (single 68 SSkinModel)
    )
   )
  )
  (ifnz 72
   (reuse 72 0
    (alloc 72 (n 4) 0
     (single 72 SSkinAnim)
    )
   )
  )
  (ifnz 76
   (alloc 76 (n 4) 0
    (raw 76 1 16 (v SSkinInstance (m 68 1) (m 0 0) (t 4 i)))
   )
  )
 )
 (load SSkinShaders 20 (full)
  (ifnz 0
   (alloc 0 (n 128) 0
    (raw 0 1 1 (v SSkinShaders (m 12 0) (t 4 i)))
   )
  )
  (ifnz 4
   (alloc 4 (n 4) 0
    (raw 4 1 1 (v SSkinShaders (m 12 0) (t 4 i)))
   )
  )
  (ifnz 8
   (alloc 8 (n 4) 0
    (raw 8 1 1 (v SSkinShaders (m 16 0) (t 4 i)))
   )
  )
 )
 (load SSkinModel 16 (full)
  (ifnz 8
   (alloc 8 (n 4) 0
    (raw 8 1 16 (v SSkinModel (m 0 0) (t 4 i)))
   )
  )
  (ifnz 12
   (alloc 12 (n 2) 0
    (raw 12 1 2 (v SSkinModel (m 4 0) (t 4 i)))
   )
  )
 )
 (load SSkinAnim 16 (full)
  (ifnz 12
   (alloc 12 (n 128) 0
    (raw 12 1 4 (op * (op * (n 4) (v SSkinAnim (m 4 0) (t 4 i))) (v SSkinAnim (m 8 0) (t 4 i))))
   )
  )
 )
 (load GfxShadowGeometry 12 (full)
  (ifnz 4
   (alloc 4 (n 2) 0
    (raw 4 1 2 (v GfxShadowGeometry (m 0 0) (t 2 u)))
   )
  )
  (ifnz 8
   (alloc 8 (n 2) 0
    (raw 8 1 2 (v GfxShadowGeometry (m 2 0) (t 2 u)))
   )
  )
 )
 (load GfxLightRegion 8 (full)
  (ifnz 4
   (alloc 4 (n 4) 0
    (arr 4 1 GfxLightRegionHull 1 (v GfxLightRegion (m 0 0) (t 4 u)))
   )
  )
 )
 (load GfxLightRegionHull 80 (full)
  (ifnz 76
   (alloc 76 (n 4) 0
    (raw 76 1 20 (v GfxLightRegionHull (m 72 0) (t 4 u)))
   )
  )
 )
 (load GfxWorldDpvsStatic 116 (full)
  (block 1
   (ifnz 48
    (alloc 48 (n 128) 0
     (raw 48 1 1 (v GfxWorldDpvsStatic (m 40 0) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 52
    (alloc 52 (n 128) 0
     (raw 52 1 1 (v GfxWorldDpvsStatic (m 40 0) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 56
    (alloc 56 (n 128) 0
     (raw 56 1 1 (v GfxWorldDpvsStatic (m 40 0) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 60
    (alloc 60 (n 128) 0
     (raw 60 1 1 (v GfxWorldDpvsStatic (m 44 0) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 64
    (alloc 64 (n 128) 0
     (raw 64 1 1 (v GfxWorldDpvsStatic (m 44 0) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 68
    (alloc 68 (n 128) 0
     (raw 68 1 1 (v GfxWorldDpvsStatic (m 44 0) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 72
    (alloc 72 (n 128) 0
     (raw 72 1 1 (v GfxWorldDpvsStatic (m 40 0) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 76
    (alloc 76 (n 128) 0
     (raw 76 1 1 (v GfxWorldDpvsStatic (m 44 0) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 100
    (alloc 100 (n 128) 0
     (raw 100 1 1 (v GfxWorldDpvsStatic (m 44 0) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 104
    (alloc 104 (n 128) 0
     (raw 104 1 1 (v GfxWorldDpvsStatic (m 44 0) (t 4 u)))
    )
   )
  )
  (ifnz 108
   (alloc 108 (n 128) 0
    (raw 108 1 1 (v GfxWorldDpvsStatic (m 40 0) (t 4 u)))
   )
  )
  (ifnz 80
   (alloc 80 (n 2) 0
    (raw 80 1 2 (v GfxWorldDpvsStatic (m 4 0) (t 4 u)))
   )
  )
  (ifnz 84
   (alloc 84 (n 4) 0
    (raw 84 1 36 (v GfxWorldDpvsStatic (m 0 0) (t 4 u)))
   )
  )
  (ifnz 88
   (alloc 88 (n 16) 0
    (arr 88 1 GfxSurface 1 (v GfxWorld (m 16 0) (t 4 i)))
   )
  )
  (ifnz 92
   (alloc 92 (n 4) 0
    (arr 92 1 GfxStaticModelDrawInst 1 (v GfxWorldDpvsStatic (m 0 0) (t 4 u)))
   )
  )
  (block 1
   (ifnz 96
    (alloc 96 (n 4) 0
     (raw 96 1 8 (v GfxWorldDpvsStatic (m 4 0) (t 4 u)))
    )
   )
  )
 )
 (load GfxSurface 80 (full)
  (ifnz 48
   (assetload 48 Material)
  )
 )
 (load GfxStaticModelDrawInst 152 (full)
  (ifnz 56
   (assetload 56 XModel)
  )
  (arr 104 0 GfxStaticModelLmapVertexInfo 0 (n 4))
 )
 (load GfxStaticModelLmapVertexInfo 12 (full)
  (ifnz 0
   (alloc 0 (n 4) 0
    (raw 0 1 4 (v GfxStaticModelLmapVertexInfo (m 8 0) (t 2 u)))
   )
  )
 )
 (load GfxWorldDpvsDynamic 52 (full)
  (block 1
   (ifnz 16
    (alloc 16 (n 4) 0
     (raw 16 1 4 (op * (v GfxWorldDpvsDynamic (m 0 0) (i (n 0) 4) (t 4 u)) (v GfxWorld (m 372 0) (m 0 0) (t 4 i))))
    )
   )
  )
  (block 1
   (ifnz 20
    (alloc 20 (n 4) 0
     (raw 20 1 4 (op * (v GfxWorldDpvsDynamic (m 0 0) (i (n 1) 4) (t 4 u)) (v GfxWorld (m 372 0) (m 0 0) (t 4 i))))
    )
   )
  )
  (block 1
   (ifnz 24
    (alloc 24 (n 16) 0
     (raw 24 1 1 (op * (n 32) (v GfxWorldDpvsDynamic (m 0 0) (i (n 0) 4) (t 4 u))))
    )
   )
  )
  (block 1
   (ifnz 28
    (alloc 28 (n 16) 0
     (raw 28 1 1 (op * (n 32) (v GfxWorldDpvsDynamic (m 0 0) (i (n 0) 4) (t 4 u))))
    )
   )
  )
  (block 1
   (ifnz 32
    (alloc 32 (n 16) 0
     (raw 32 1 1 (op * (n 32) (v GfxWorldDpvsDynamic (m 0 0) (i (n 0) 4) (t 4 u))))
    )
   )
  )
  (block 1
   (ifnz 36
    (alloc 36 (n 16) 0
     (raw 36 1 1 (op * (n 32) (v GfxWorldDpvsDynamic (m 0 0) (i (n 1) 4) (t 4 u))))
    )
   )
  )
  (block 1
   (ifnz 40
    (alloc 40 (n 16) 0
     (raw 40 1 1 (op * (n 32) (v GfxWorldDpvsDynamic (m 0 0) (i (n 1) 4) (t 4 u))))
    )
   )
  )
  (block 1
   (ifnz 44
    (alloc 44 (n 16) 0
     (raw 44 1 1 (op * (n 32) (v GfxWorldDpvsDynamic (m 0 0) (i (n 1) 4) (t 4 u))))
    )
   )
  )
 )
 (load GfxWaterBuffer 8 (full)
  (ifnz 4
   (alloc 4 (n 4) 0
    (raw 4 1 16 (op / (v GfxWaterBuffer (m 0 0) (t 4 u)) (n 16)))
   )
  )
 )
 (load GfxWorld 1028 (full)
  (push 5)
  (xstring 0)
  (xstring 4)
  (embedded 20 GfxWorldStreamInfo 0)
  (xstring 36)
  (ifnz 256
   (reuse 256 0
    (alloc 256 (n 16) 0
     (single 256 GfxLight)
    )
   )
  )
  (ifnz 272
   (alloc 272 (n 4) 0
    (raw 272 1 32 (v GfxWorld (m 268 0) (t 4 u)))
   )
  )
  (ifnz 280
   (alloc 280 (n 4) 0
    (raw 280 1 16 (v GfxWorld (m 276 0) (t 4 u)))
   )
  )
  (ifnz 288
   (alloc 288 (n 4) 0
    (raw 288 1 16 (v GfxWorld (m 284 0) (t 4 u)))
   )
  )
  (ifnz 296
   (alloc 296 (n 4) 0
    (raw 296 1 24 (v GfxWorld (m 292 0) (t 4 u)))
   )
  )
  (ifnz 304
   (alloc 304 (n 4) 0
    (raw 304 1 16 (v GfxWorld (m 300 0) (t 4 u)))
   )
  )
  (ifnz 312
   (alloc 312 (n 4) 0
    (raw 312 1 100 (v GfxWorld (m 308 0) (t 4 u)))
   )
  )
  (ifnz 320
   (alloc 320 (n 4) 0
    (raw 320 1 16 (v GfxWorld (m 316 0) (t 4 u)))
   )
  )
  (ifnz 328
   (alloc 328 (n 4) 0
    (raw 328 1 48 (v GfxWorld (m 324 0) (t 4 u)))
   )
  )
  (ifnz 336
   (alloc 336 (n 4) 0
    (raw 336 1 16 (v GfxWorld (m 332 0) (t 4 u)))
   )
  )
  (ifnz 344
   (alloc 344 (n 4) 0
    (raw 344 1 36 (v GfxWorld (m 340 0) (t 4 u)))
   )
  )
  (ifnz 352
   (alloc 352 (n 4) 0
    (raw 352 1 16 (v GfxWorld (m 348 0) (t 4 u)))
   )
  )
  (embedded 372 GfxWorldDpvsPlanes 0)
  (ifnz 392
   (alloc 392 (n 4) 0
    (arr 392 1 GfxCell 1 (v GfxWorld (m 372 0) (m 0 0) (t 4 i)))
   )
  )
  (embedded 396 GfxWorldDraw 0)
  (embedded 464 GfxLightGrid 0)
  (ifnz 540
   (alloc 540 (n 16) 0
    (raw 540 1 64 (v GfxWorld (m 536 0) (t 4 i)))
   )
  )
  (ifnz 576
   (alloc 576 (n 4) 0
    (arr 576 1 MaterialMemory 1 (v GfxWorld (m 572 0) (t 4 i)))
   )
  )
  (embedded 580 sunflare_t 0)
  (ifnz 740
   (assetload 740 GfxImage)
  )
  (block 1
   (ifnz 744
    (alloc 744 (n 4) 0
     (raw 744 1 4 (op * (v GfxWorld (m 372 0) (m 0 0) (t 4 i)) (op / (op + (v GfxWorld (m 372 0) (m 0 0) (t 4 i)) (n 31)) (n 32))))
    )
   )
  )
  (block 1
   (ifnz 748
    (alloc 748 (n 4) 0
     (raw 748 1 8 (v GfxWorld (m 900 0) (m 8 0) (i (n 0) 4) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 752
    (alloc 752 (n 4) 0
     (raw 752 1 4 (v GfxWorld (m 900 0) (m 8 0) (i (n 1) 4) (t 4 u)))
    )
   )
  )
  (block 1
   (ifnz 756
    (alloc 756 (n 4) 0
     (raw 756 1 4 (op * (op - (op - (v GfxWorld (m 264 0) (t 4 u)) (v GfxWorld (m 260 0) (t 4 u))) (n 1)) (n 8192)))
    )
   )
  )
  (block 1
   (ifnz 760
    (alloc 760 (n 4) 0
     (raw 760 1 4 (op * (v GfxWorld (m 900 0) (m 8 0) (i (n 0) 4) (t 4 u)) (op - (op - (v GfxWorld (m 264 0) (t 4 u)) (v GfxWorld (m 260 0) (t 4 u))) (n 1))))
    )
   )
  )
  (block 1
   (ifnz 764
    (alloc 764 (n 4) 0
     (raw 764 1 4 (op * (v GfxWorld (m 900 0) (m 8 0) (i (n 1) 4) (t 4 u)) (op - (op - (v GfxWorld (m 264 0) (t 4 u)) (v GfxWorld (m 260 0) (t 4 u))) (n 1))))
    )
   )
  )
  (ifnz 772
   (alloc 772 (n 16) 0
    (arr 772 1 SSkinInstance 1 (v GfxWorld (m 768 0) (t 4 u)))
   )
  )
  (ifnz 776
   (alloc 776 (n 4) 0
    (arr 776 1 GfxShadowGeometry 1 (v GfxWorld (m 264 0) (t 4 u)))
   )
  )
  (ifnz 780
   (alloc 780 (n 4) 0
    (arr 780 1 GfxLightRegion 1 (v GfxWorld (m 264 0) (t 4 u)))
   )
  )
  (embedded 784 GfxWorldDpvsStatic 0)
  (embedded 900 GfxWorldDpvsDynamic 0)
  (arr 956 0 GfxWaterBuffer 0 (n 2))
  (ifnz 972
   (assetload 972 Material)
  )
  (ifnz 976
   (assetload 976 Material)
  )
  (ifnz 980
   (assetload 980 Material)
  )
  (ifnz 984
   (assetload 984 Material)
  )
  (ifnz 992
   (alloc 992 (n 4) 0
    (raw 992 1 68 (v GfxWorld (m 988 0) (t 4 u)))
   )
  )
  (ifnz 1000
   (alloc 1000 (n 4) 0
    (raw 1000 1 24 (v GfxWorld (m 996 0) (t 4 u)))
   )
  )
  (ifnz 1012
   (alloc 1012 (n 4) 0
    (raw 1012 1 56 (v GfxWorld (m 1004 0) (t 4 u)))
   )
  )
  (ifnz 1016
   (alloc 1016 (n 4) 0
    (raw 1016 1 32 (v GfxWorld (m 1008 0) (t 4 u)))
   )
  )
  (pop)
 )
 (loadptr GfxWorld 1 (n 4))
)
(asset Glasses
 (array Glass 140 Glass)
 (load Glass 140 (full)
  (ifnz 16
   (reuse 16 0
    (alloc 16 (n 4) 0
     (single 16 GlassDef)
    )
   )
  )
  (ifnz 80
   (alloc 80 (n 4) 0
    (raw 80 1 8 (v Glass (m 77 0) (t 1 i)))
   )
  )
 )
 (load GlassDef 60 (full)
  (xstring 0)
  (ifnz 28
   (assetload 28 Material)
  )
  (ifnz 32
   (assetload 32 Material)
  )
  (ifnz 36
   (assetload 36 Material)
  )
  (xstring 40)
  (xstring 44)
  (xstring 48)
  (ifnz 52
   (assetload 52 FxEffectDef)
  )
  (ifnz 56
   (assetload 56 FxEffectDef)
  )
 )
 (load Glasses 56 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 4) 0
    (arr 8 1 Glass 1 (v Glasses (m 4 0) (t 4 u)))
   )
  )
  (pop)
 )
 (loadptr Glasses 1 (n 4))
)
(asset KeyValuePairs
 (array KeyValuePair 12 KeyValuePair)
 (load KeyValuePair 12 (full)
  (xstring 8)
 )
 (load KeyValuePairs 12 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 4) 0
    (arr 8 1 KeyValuePair 1 (v KeyValuePairs (m 4 0) (t 4 u)))
   )
  )
  (pop)
 )
 (loadptr KeyValuePairs 1 (n 4))
)
(asset LeaderboardDef
 (array LbColumnDef 44 LbColumnDef)
 (load LbColumnDef 44 (full)
  (xstring 0)
  (xstring 16)
  (xstring 32)
 )
 (load LeaderboardDef 36 (full)
  (push 5)
  (xstring 0)
  (ifnz 24
   (alloc 24 (n 4) 0
    (arr 24 1 LbColumnDef 1 (v LeaderboardDef (m 8 0) (t 4 i)))
   )
  )
  (pop)
 )
 (loadptr LeaderboardDef 1 (n 4))
)
(asset LocalizeEntry
 (load LocalizeEntry 8 (full)
  (push 5)
  (xstring 0)
  (xstring 4)
  (pop)
 )
 (loadptr LocalizeEntry 1 (n 4))
)
(asset MapEnts
 (load MapTriggers 24 (full)
  (ifnz 4
   (reuse 4 0
    (alloc 4 (n 4) 0
     (raw 4 1 8 (v MapTriggers (m 0 0) (t 4 u)))
    )
   )
  )
  (ifnz 12
   (reuse 12 0
    (alloc 12 (n 4) 0
     (raw 12 1 32 (v MapTriggers (m 8 0) (t 4 u)))
    )
   )
  )
  (ifnz 20
   (reuse 20 0
    (alloc 20 (n 4) 0
     (raw 20 1 20 (v MapTriggers (m 16 0) (t 4 u)))
    )
   )
  )
 )
 (load MapEnts 36 (full)
  (push 5)
  (xstring 0)
  (ifnz 4
   (alloc 4 (n 1) 0
    (raw 4 1 1 (v MapEnts (m 8 0) (t 4 i)))
   )
  )
  (embedded 12 MapTriggers 0)
  (pop)
 )
 (loadptr MapEnts 1 (n 4))
)
(asset Material
 (array MaterialTextureDef 16 MaterialTextureDef)
 (load MaterialInfo 48 (full)
  (xstring 0)
 )
 (load MaterialTextureDef 16 (full)
  (ifnz 12
   (assetload 12 GfxImage)
  )
 )
 (load Material 112 (full)
  (push 5)
  (embedded 0 MaterialInfo 0)
  (ifnz 92
   (assetload 92 MaterialTechniqueSet)
  )
  (ifnz 96
   (reuse 96 0
    (alloc 96 (n 4) 0
     (arr 96 1 MaterialTextureDef 1 (v Material (m 84 0) (t 1 u)))
    )
   )
  )
  (ifnz 100
   (reuse 100 0
    (alloc 100 (n 16) 0
     (raw 100 1 32 (v Material (m 85 0) (t 1 u)))
    )
   )
  )
  (ifnz 104
   (reuse 104 0
    (alloc 104 (n 8) 0
     (raw 104 1 20 (v Material (m 86 0) (t 1 u)))
    )
   )
  )
  (ifnz 108
   (assetload 108 Material)
  )
  (pop)
 )
 (loadptr Material 1 (n 4))
)
(asset MaterialTechniqueSet
 (ptrarray MaterialTechnique 32 MaterialTechnique 1 0 1 (n 4))
 (array MaterialPass 24 MaterialPass)
 (array MaterialShaderArgument 12 MaterialShaderArgument)
 (load MaterialTechnique 32 (partial 8)
  (arr 8 0 MaterialPass 1 (v MaterialTechnique (m 6 0) (t 2 u)))
  (xstring 0)
 )
 (load MaterialPass 24 (full)
  (ifnz 4
   (reuse 4 0
    (alloc 4 (n 4) 0
     (single 4 MaterialVertexShader)
    )
   )
  )
  (ifnz 0
   (reuse 0 0
    (alloc 0 (n 4) 0
     (raw 0 1 116 (n 1))
    )
   )
  )
  (ifnz 8
   (reuse 8 0
    (alloc 8 (n 4) 0
     (single 8 MaterialPixelShader)
    )
   )
  )
  (ifnz 20
   (alloc 20 (n 4) 0
    (arr 20 1 MaterialShaderArgument 1 (op + (op + (v MaterialPass (m 12 0) (t 1 u)) (v MaterialPass (m 13 0) (t 1 u))) (v MaterialPass (m 14 0) (t 1 u))))
   )
  )
 )
 (load MaterialVertexShader 16 (full)
  (xstring 0)
  (embedded 4 MaterialVertexShaderProgram 0)
 )
 (load MaterialVertexShaderProgram 12 (full)
  (embedded 4 GfxVertexShaderLoadDef 0)
 )
 (load GfxVertexShaderLoadDef 8 (full)
  (ifnz 0
   (alloc 0 (n 1) 0
    (raw 0 1 1 (v GfxVertexShaderLoadDef (m 4 0) (t 4 u)))
   )
  )
 )
 (load MaterialPixelShader 16 (full)
  (xstring 0)
  (embedded 4 MaterialPixelShaderProgram 0)
 )
 (load MaterialPixelShaderProgram 12 (full)
  (embedded 4 GfxPixelShaderLoadDef 0)
 )
 (load GfxPixelShaderLoadDef 8 (full)
  (ifnz 0
   (alloc 0 (n 1) 0
    (raw 0 1 1 (v GfxPixelShaderLoadDef (m 4 0) (t 4 u)))
   )
  )
 )
 (load MaterialShaderArgument 12 (full)
  (embedded 8 MaterialArgumentDef 0)
 )
 (load MaterialArgumentDef 4 (full)
  (chain
   (case (op || (op == (v MaterialShaderArgument (m 0 0) (t 2 i)) (n 1)) (op == (v MaterialShaderArgument (m 0 0) (t 2 i)) (n 7)))
    (ifnz 0
     (reuse 0 0
      (alloc 0 (n 4) 0
       (raw 0 1 16 (n 1))
      )
     )
    )
   )
  )
 )
 (load MaterialTechniqueSet 152 (full)
  (push 5)
  (xstring 0)
  (ptrarr 8 0 MaterialTechnique 0 (n 36))
  (pop)
 )
 (loadptr MaterialTechniqueSet 1 (n 4))
)
(asset MemoryBlock
 (load MemoryBlockData 4 (full)
  (chain
   (case (v MemoryBlock (m 4 0) (t 1 u))
    (block 1
     (ifnz 0
      (alloc 0 (v MemoryBlock (m 12 0) (t 4 u)) 0
       (raw 0 1 1 (v MemoryBlock (m 8 0) (t 4 u)))
      )
     )
    )
   )
   (case (v MemoryBlock (m 5 0) (t 1 u))
    (block 2
     (ifnz 0
      (alloc 0 (v MemoryBlock (m 12 0) (t 4 u)) 0
       (raw 0 1 1 (v MemoryBlock (m 8 0) (t 4 u)))
      )
     )
    )
   )
   (case (v MemoryBlock (m 6 0) (t 1 u))
    (block 7
     (ifnz 0
      (alloc 0 (v MemoryBlock (m 12 0) (t 4 u)) 0
       (raw 0 1 1 (v MemoryBlock (m 8 0) (t 4 u)))
      )
     )
    )
   )
  )
 )
 (load MemoryBlock 20 (full)
  (push 5)
  (xstring 0)
  (embedded 16 MemoryBlockData 0)
  (pop)
 )
 (loadptr MemoryBlock 1 (n 4))
)
(asset menuDef_t
 (ptrarray itemDef_s 272 itemDef_s 0 0 1 (n 8))
 (ptrarray animParamsDef_t 108 animParamsDef_t 0 0 1 (n 4))
 (array expressionRpn 12 expressionRpn)
 (array MenuRow 24 MenuRow)
 (array MenuCell 12 MenuCell)
 (load windowDef_t 164 (full)
  (xstring 0)
  (xstring 52)
  (ifnz 160
   (assetload 160 Material)
  )
 )
 (load GenericEventHandler 12 (full)
  (xstring 0)
  (ifnz 4
   (alloc 4 (n 4) 0
    (single 4 GenericEventScript)
   )
  )
  (ifnz 8
   (alloc 8 (n 4) 0
    (single 8 GenericEventHandler)
   )
  )
 )
 (load GenericEventScript 44 (full)
  (ifnz 0
   (alloc 0 (n 4) 0
    (single 0 ScriptCondition)
   )
  )
  (embedded 4 ExpressionStatement 0)
  (xstring 28)
  (ifnz 40
   (alloc 40 (n 4) 0
    (single 40 GenericEventScript)
   )
  )
 )
 (load ScriptCondition 16 (full)
  (ifnz 12
   (alloc 12 (n 4) 0
    (single 12 ScriptCondition)
   )
  )
 )
 (load ExpressionStatement 16 (full)
  (xstring 0)
  (ifnz 12
   (reuse 12 0
    (alloc 12 (n 4) 0
     (arr 12 1 expressionRpn 1 (v ExpressionStatement (m 8 0) (t 4 i)))
    )
   )
  )
 )
 (load expressionRpn 12 (full)
  (embedded 4 expressionRpnDataUnion 0)
 )
 (load expressionRpnDataUnion 8 (full)
  (chain
   (case (op == (v expressionRpn (m 0 0) (t 4 i)) (n 0))
    (embedded 0 Operand 0)
   )
  )
 )
 (load Operand 8 (full)
  (embedded 4 operandInternalDataUnion 0)
 )
 (load operandInternalDataUnion 4 (full)
  (chain
   (case (op == (v Operand (m 0 0) (t 4 i)) (n 2))
    (xstring 0)
   )
  )
 )
 (load ItemKeyHandler 12 (full)
  (ifnz 4
   (alloc 4 (n 4) 0
    (single 4 GenericEventScript)
   )
  )
  (ifnz 8
   (alloc 8 (n 4) 0
    (single 8 ItemKeyHandler)
   )
  )
 )
 (load itemDef_s 272 (full)
  (embedded 0 windowDef_t 0)
  (xstring 176)
  (xstring 180)
  (xstring 184)
  (xstring 188)
  (embedded 196 itemDefData_t 0)
  (ifnz 204
   (alloc 204 (n 4) 0
    (single 204 rectData_s)
   )
  )
  (embedded 208 ExpressionStatement 0)
  (embedded 240 ExpressionStatement 0)
  (ifnz 260
   (alloc 260 (n 4) 0
    (single 260 GenericEventHandler)
   )
  )
  (ifnz 264
   (alloc 264 (n 4) 0
    (single 264 UIAnimInfo)
   )
  )
 )
 (load itemDefData_t 4 (full)
  (chain
   (case (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op == (v itemDef_s (m 164 0) (t 4 i)) (n 1)) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 3))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 4))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 5))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 7))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 8))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 9))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 10))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 11))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 12))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 13))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 14))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 15))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 16))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 18))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 20))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 22)))
    (ifnz 0
     (alloc 0 (n 4) 0
      (single 0 textDef_s)
     )
    )
   )
   (case (op == (v itemDef_s (m 164 0) (t 4 i)) (n 2))
    (ifnz 0
     (alloc 0 (n 4) 0
      (single 0 imageDef_s)
     )
    )
   )
   (case (op || (op == (v itemDef_s (m 164 0) (t 4 i)) (n 19)) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 21)))
    (ifnz 0
     (alloc 0 (n 4) 0
      (single 0 focusItemDef_s)
     )
    )
   )
   (case (op == (v itemDef_s (m 164 0) (t 4 i)) (n 6))
    (ifnz 0
     (alloc 0 (n 4) 0
      (single 0 ownerDrawDef_s)
     )
    )
   )
  )
 )
 (load textDef_s 140 (full)
  (xstring 128)
  (ifnz 132
   (alloc 132 (n 4) 0
    (single 132 textExp_s)
   )
  )
  (embedded 136 textDefData_t 0)
 )
 (load textExp_s 16 (full)
  (embedded 0 ExpressionStatement 0)
 )
 (load textDefData_t 4 (full)
  (chain
   (case (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op || (op == (v itemDef_s (m 164 0) (t 4 i)) (n 3)) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 4))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 5))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 7))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 8))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 9))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 10))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 11))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 12))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 13))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 14))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 16))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 20))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 21))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 22))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 30)))
    (ifnz 0
     (alloc 0 (n 4) 0
      (single 0 focusItemDef_s)
     )
    )
   )
   (case (op == (v itemDef_s (m 164 0) (t 4 i)) (n 15))
    (ifnz 0
     (alloc 0 (n 4) 0
      (raw 0 1 8 (n 1))
     )
    )
   )
  )
 )
 (load focusItemDef_s 24 (full)
  (xstring 0)
  (xstring 4)
  (xstring 8)
  (xstring 12)
  (ifnz 16
   (alloc 16 (n 4) 0
    (single 16 ItemKeyHandler)
   )
  )
  (embedded 20 focusDefData_t 0)
 )
 (load focusDefData_t 4 (full)
  (chain
   (case (op == (v itemDef_s (m 164 0) (t 4 i)) (n 4))
    (ifnz 0
     (alloc 0 (n 4) 0
      (single 0 listBoxDef_s)
     )
    )
   )
   (case (op == (v itemDef_s (m 164 0) (t 4 i)) (n 10))
    (ifnz 0
     (alloc 0 (n 4) 0
      (single 0 multiDef_s)
     )
    )
   )
   (case (op == (v itemDef_s (m 164 0) (t 4 i)) (n 22))
    (ifnz 0
     (alloc 0 (n 4) 0
      (single 0 profileMultiDef_s)
     )
    )
   )
   (case (op || (op || (op || (op || (op || (op || (op || (op || (op == (v itemDef_s (m 164 0) (t 4 i)) (n 5)) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 7))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 8))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 9))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 12))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 13))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 14))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 16))) (op == (v itemDef_s (m 164 0) (t 4 i)) (n 30)))
    (ifnz 0
     (alloc 0 (n 4) 0
      (raw 0 1 36 (n 1))
     )
    )
   )
   (case (op == (v itemDef_s (m 164 0) (t 4 i)) (n 11))
    (ifnz 0
     (alloc 0 (n 4) 0
      (single 0 enumDvarDef_s)
     )
    )
   )
  )
 )
 (load listBoxDef_s 668 (full)
  (ifnz 640
   (assetload 640 Material)
  )
  (ifnz 644
   (assetload 644 Material)
  )
  (ifnz 648
   (assetload 648 Material)
  )
  (ifnz 656
   (alloc 656 (n 4) 0
    (arr 656 1 MenuRow 1 (v listBoxDef_s (m 660 0) (t 4 i)))
   )
  )
 )
 (load MenuRow 24 (full)
  (ifnz 0
   (alloc 0 (n 4) 0
    (arr 0 1 MenuCell 1 (v listBoxDef_s (m 28 0) (t 4 i)))
   )
  )
  (ifnz 4
   (alloc 4 (n 1) 0
    (raw 4 1 1 (n 32))
   )
  )
  (ifnz 8
   (alloc 8 (n 1) 0
    (raw 8 1 1 (n 32))
   )
  )
 )
 (load MenuCell 12 (full)
  (ifnz 8
   (alloc 8 (n 1) 0
    (raw 8 1 1 (v MenuCell (m 4 0) (t 4 i)))
   )
  )
 )
 (load multiDef_s 396 (full)
  (xstrarr 0 0 0 (n 32))
  (xstrarr 128 0 0 (n 32))
 )
 (load profileMultiDef_s 396 (full)
  (xstrarr 0 0 0 (n 32))
  (xstrarr 128 0 0 (n 32))
 )
 (load enumDvarDef_s 4 (full)
  (xstring 0)
 )
 (load imageDef_s 16 (full)
  (embedded 0 ExpressionStatement 0)
 )
 (load ownerDrawDef_s 16 (full)
  (embedded 0 ExpressionStatement 0)
 )
 (load rectData_s 64 (full)
  (embedded 0 ExpressionStatement 0)
  (embedded 16 ExpressionStatement 0)
  (embedded 32 ExpressionStatement 0)
  (embedded 48 ExpressionStatement 0)
 )
 (load UIAnimInfo 236 (full)
  (ifnz 4
   (alloc 4 (n 4) 0
    (ptrarr 4 1 animParamsDef_t 1 (v UIAnimInfo (m 0 0) (t 4 i)))
   )
  )
 )
 (load animParamsDef_t 108 (full)
  (xstring 0)
  (ifnz 104
   (alloc 104 (n 4) 0
    (single 104 GenericEventHandler)
   )
  )
 )
 (load menuDef_t 400 (full)
  (push 5)
  (embedded 0 windowDef_t 0)
  (xstring 164)
  (ifnz 268
   (alloc 268 (n 4) 0
    (single 268 GenericEventHandler)
   )
  )
  (ifnz 272
   (alloc 272 (n 4) 0
    (single 272 ItemKeyHandler)
   )
  )
  (embedded 276 ExpressionStatement 0)
  (xstring 312)
  (xstring 316)
  (embedded 360 ExpressionStatement 0)
  (embedded 376 ExpressionStatement 0)
  (ifnz 392
   (alloc 392 (n 4) 0
    (ptrarr 392 1 itemDef_s 1 (v menuDef_t (m 176 0) (t 4 i)))
   )
  )
  (pop)
 )
 (loadptr menuDef_t 1 (n 8))
)
(asset MenuList
 (ptrarray menuDef_t 400 menuDef_t 0 1 1 (n 8))
 (load MenuList 12 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 4) 0
    (ptrarr 8 1 menuDef_t 1 (v MenuList (m 4 0) (t 4 i)))
   )
  )
  (pop)
 )
 (loadptr MenuList 1 (n 4))
)
(asset PhysConstraints
 (array PhysConstraint 168 PhysConstraint)
 (load PhysConstraint 168 (full)
  (xstring 20)
  (xstring 36)
  (ifnz 140
   (assetload 140 Material)
  )
 )
 (load PhysConstraints 2696 (full)
  (push 5)
  (xstring 0)
  (arr 8 0 PhysConstraint 0 (n 16))
  (pop)
 )
 (loadptr PhysConstraints 1 (n 4))
)
(asset PhysPreset
 (load PhysPreset 84 (full)
  (push 5)
  (xstring 0)
  (xstring 28)
  (pop)
 )
 (loadptr PhysPreset 1 (n 4))
)
(asset Qdb
 (load Qdb 12 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 32) 0
    (raw 8 1 1 (op + (v Qdb (m 4 0) (t 4 i)) (n 1)))
   )
  )
  (pop)
 )
 (loadptr Qdb 1 (n 4))
)
(asset RawFile
 (load RawFile 12 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 16) 0
    (raw 8 1 1 (op + (v RawFile (m 4 0) (t 4 i)) (n 1)))
   )
  )
  (pop)
 )
 (loadptr RawFile 1 (n 4))
)
(asset ScriptParseTree
 (load ScriptParseTree 12 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 32) 0
    (raw 8 1 1 (op + (v ScriptParseTree (m 4 0) (t 4 i)) (n 1)))
   )
  )
  (pop)
 )
 (loadptr ScriptParseTree 1 (n 4))
)
(asset SkinnedVertsDef
 (load SkinnedVertsDef 8 (full)
  (push 5)
  (xstring 0)
  (pop)
 )
 (loadptr SkinnedVertsDef 1 (n 4))
)
(asset Slug
 (load Slug 12 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 32) 0
    (raw 8 1 1 (op + (v Slug (m 4 0) (t 4 i)) (n 1)))
   )
  )
  (pop)
 )
 (loadptr Slug 1 (n 4))
)
(asset SndBank
 (array SndAliasList 20 SndAliasList)
 (array SndAlias 96 SndAlias)
 (array SndDuck 76 SndDuck)
 (load SndAliasList 20 (full)
  (xstring 0)
  (ifnz 8
   (reuse 8 0
    (alloc 8 (n 4) 0
     (arr 8 1 SndAlias 1 (v SndAliasList (m 12 0) (t 4 i)))
    )
   )
  )
 )
 (load SndAlias 96 (full)
  (xstring 0)
  (xstring 8)
  (xstring 12)
  (xstring 20)
 )
 (load SndDuck 76 (full)
  (ifnz 64
   (alloc 64 (n 16) 0
    (raw 64 1 4 (n 32))
   )
  )
  (ifnz 68
   (alloc 68 (n 16) 0
    (raw 68 1 4 (n 32))
   )
  )
 )
 (load SndRuntimeAssetBank 2338 (full)
  (xstring 0)
  (xstring 4)
 )
 (load SndLoadedAssets 28 (full)
  (xstring 0)
  (xstring 4)
  (ifnz 16
   (alloc 16 (n 4) 0
    (raw 16 1 20 (v SndLoadedAssets (m 12 0) (t 4 u)))
   )
  )
  (ifnz 24
   (alloc 24 (n 2048) 0
    (raw 24 1 1 (v SndLoadedAssets (m 20 0) (t 4 u)))
   )
  )
 )
 (load SndBank 4756 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 4) 0
    (arr 8 1 SndAliasList 1 (v SndBank (m 4 0) (t 4 u)))
   )
  )
  (ifnz 12
   (alloc 12 (n 4) 0
    (raw 12 1 4 (v SndBank (m 4 0) (t 4 u)))
   )
  )
  (ifnz 20
   (alloc 20 (n 4) 0
    (raw 20 1 100 (v SndBank (m 16 0) (t 4 u)))
   )
  )
  (ifnz 28
   (alloc 28 (n 4) 0
    (arr 28 1 SndDuck 1 (v SndBank (m 24 0) (t 4 u)))
   )
  )
  (embedded 32 SndRuntimeAssetBank 0)
  (embedded 2370 SndRuntimeAssetBank 0)
  (embedded 4708 SndLoadedAssets 0)
  (ifnz 4740
   (alloc 4740 (n 4) 0
    (raw 4740 1 8 (v SndBank (m 4736 0) (t 4 u)))
   )
  )
  (pop)
 )
 (loadptr SndBank 1 (n 4))
)
(asset SndDriverGlobals
 (load SndDriverGlobals 68 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 4) 0
    (raw 8 1 80 (v SndDriverGlobals (m 4 0) (t 4 u)))
   )
  )
  (ifnz 16
   (alloc 16 (n 4) 0
    (raw 16 1 100 (v SndDriverGlobals (m 12 0) (t 4 u)))
   )
  )
  (ifnz 24
   (alloc 24 (n 4) 0
    (raw 24 1 60 (v SndDriverGlobals (m 20 0) (t 4 u)))
   )
  )
  (ifnz 32
   (alloc 32 (n 4) 0
    (raw 32 1 36 (v SndDriverGlobals (m 28 0) (t 4 u)))
   )
  )
  (ifnz 40
   (alloc 40 (n 4) 0
    (raw 40 1 36 (v SndDriverGlobals (m 36 0) (t 4 u)))
   )
  )
  (ifnz 48
   (alloc 48 (n 4) 0
    (raw 48 1 240 (v SndDriverGlobals (m 44 0) (t 4 u)))
   )
  )
  (ifnz 56
   (alloc 56 (n 4) 0
    (raw 56 1 60 (v SndDriverGlobals (m 52 0) (t 4 u)))
   )
  )
  (ifnz 64
   (alloc 64 (n 4) 0
    (raw 64 1 100 (v SndDriverGlobals (m 60 0) (t 4 u)))
   )
  )
  (pop)
 )
 (loadptr SndDriverGlobals 1 (n 4))
)
(asset SndPatch
 (load SndPatch 12 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (alloc 8 (n 4) 0
    (raw 8 1 4 (v SndPatch (m 4 0) (t 4 u)))
   )
  )
  (pop)
 )
 (loadptr SndPatch 1 (n 4))
)
(asset StringTable
 (array StringTableCell 8 StringTableCell)
 (load StringTableCell 8 (full)
  (xstring 0)
 )
 (load StringTable 20 (full)
  (push 5)
  (xstring 0)
  (ifnz 12
   (alloc 12 (n 4) 0
    (arr 12 1 StringTableCell 1 (op * (v StringTable (m 4 0) (t 4 i)) (v StringTable (m 8 0) (t 4 i))))
   )
  )
  (ifnz 16
   (alloc 16 (n 2) 0
    (raw 16 1 2 (op * (v StringTable (m 4 0) (t 4 i)) (v StringTable (m 8 0) (t 4 i))))
   )
  )
  (pop)
 )
 (loadptr StringTable 1 (n 4))
)
(asset TracerDef
 (load TracerDef 128 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (assetload 8 Material)
  )
  (pop)
 )
 (loadptr TracerDef 1 (n 4))
)
(asset VehicleDef
 (ptrarray XModel 248 XModel 0 1 1 (n 4))
 (ptrarray FxEffectDef 76 FxEffectDef 0 1 1 (n 4))
 (array VehicleDriveBySound 12 VehicleDriveBySound)
 (array VehicleEngineSound 28 VehicleEngineSound)
 (load VehicleParameter 252 (full)
  (xstring 116)
 )
 (load VehicleDriveBySound 12 (full)
  (xstring 4)
 )
 (load VehicleEngine 456 (full)
  (arr 48 0 VehicleEngineSound 0 (n 5))
  (arr 188 0 VehicleEngineSound 0 (n 5))
 )
 (load VehicleEngineSound 28 (full)
  (xstring 0)
 )
 (load VehicleDef 2604 (full)
  (push 5)
  (xstring 0)
  (xstring 364)
  (xstrarr 412 0 0 (n 4))
  (xstrarr 664 0 0 (n 2))
  (xstrarr 680 0 0 (n 3))
  (xstring 700)
  (xstring 712)
  (ptrarr 748 0 XModel 0 (n 4))
  (ptrarr 772 0 XModel 0 (n 4))
  (ifnz 804
   (assetload 804 XModel)
  )
  (ifnz 808
   (assetload 808 XModel)
  )
  (ifnz 812
   (assetload 812 XModel)
  )
  (ifnz 816
   (assetload 816 XModel)
  )
  (ifnz 824
   (assetload 824 FxEffectDef)
  )
  (ptrarr 832 0 FxEffectDef 0 (n 32))
  (ifnz 960
   (assetload 960 FxEffectDef)
  )
  (xstring 968)
  (ptrarr 972 0 FxEffectDef 0 (n 4))
  (ifnz 996
   (assetload 996 FxEffectDef)
  )
  (ifnz 1004
   (assetload 1004 FxEffectDef)
  )
  (xstring 1024)
  (xstring 1028)
  (xstring 1044)
  (xstring 1104)
  (ifnz 1108
   (assetload 1108 Material)
  )
  (xstring 1112)
  (xstring 1120)
  (xstring 1128)
  (xstring 1136)
  (xstring 1144)
  (xstring 1152)
  (xstring 1160)
  (xstring 1168)
  (xstring 1176)
  (xstring 1184)
  (xstring 1192)
  (embedded 1340 VehicleParameter 0)
  (arr 1600 0 VehicleDriveBySound 0 (n 40))
  (embedded 2088 VehicleEngine 0)
  (xstring 2576)
  (pop)
 )
 (loadptr VehicleDef 1 (n 4))
)
(asset WeaponAttachment
 (load WeaponAttachment 284 (full)
  (push 5)
  (xstring 0)
  (xstring 4)
  (pop)
 )
 (loadptr WeaponAttachment 1 (n 4))
)
(asset WeaponAttachmentUnique
 (load WeaponAttachmentUnique 424 (full)
  (push 5)
  (xstring 0)
  (xstring 20)
  (xstring 28)
  (ifnz 36
   (reuse 36 0
    (alloc 36 (n 2) 0
     (raw 36 1 2 (n 32))
    )
   )
  )
  (ifnz 40
   (assetload 40 XModel)
  )
  (ifnz 44
   (assetload 44 XModel)
  )
  (ifnz 48
   (assetload 48 XModel)
  )
  (ifnz 52
   (assetload 52 XModel)
  )
  (ifnz 56
   (assetload 56 XModel)
  )
  (xstring 60)
  (xstring 64)
  (ifnz 164
   (assetload 164 WeaponCamo)
  )
  (ifnz 196
   (assetload 196 Material)
  )
  (ifnz 200
   (assetload 200 Material)
  )
  (ifnz 232
   (reuse 232 0
    (alloc 232 (n 4) 0
     (xstrarr 232 1 1 (n 88))
    )
   )
  )
  (ifnz 248
   (reuse 248 0
    (alloc 248 (n 4) 0
     (raw 248 1 4 (n 21))
    )
   )
  )
  (xstring 256)
  (xstring 260)
  (xstring 264)
  (xstring 268)
  (xstring 272)
  (xstring 276)
  (xstring 280)
  (xstring 284)
  (xstring 288)
  (xstring 292)
  (xstring 296)
  (xstring 300)
  (xstring 304)
  (xstring 308)
  (ifnz 316
   (assetload 316 FxEffectDef)
  )
  (ifnz 320
   (assetload 320 FxEffectDef)
  )
  (ifnz 324
   (assetload 324 TracerDef)
  )
  (ifnz 328
   (assetload 328 TracerDef)
  )
  (pop)
 )
 (loadptr WeaponAttachmentUnique 1 (n 4))
)
(asset WeaponCamo
 (ptrarray Material 112 Material 0 1 1 (n 4))
 (array WeaponCamoSet 20 WeaponCamoSet)
 (array WeaponCamoMaterialSet 8 WeaponCamoMaterialSet)
 (array WeaponCamoMaterial 44 WeaponCamoMaterial)
 (load WeaponCamoSet 20 (full)
  (ifnz 0
   (assetload 0 GfxImage)
  )
  (ifnz 4
   (assetload 4 GfxImage)
  )
 )
 (load WeaponCamoMaterialSet 8 (full)
  (ifnz 4
   (alloc 4 (n 4) 0
    (arr 4 1 WeaponCamoMaterial 1 (v WeaponCamoMaterialSet (m 0 0) (t 4 u)))
   )
  )
 )
 (load WeaponCamoMaterial 44 (full)
  (ifnz 4
   (alloc 4 (n 4) 0
    (ptrarr 4 1 Material 1 (v WeaponCamoMaterial (m 2 0) (t 2 u)))
   )
  )
  (ifnz 8
   (alloc 8 (n 4) 0
    (ptrarr 8 1 Material 1 (v WeaponCamoMaterial (m 2 0) (t 2 u)))
   )
  )
 )
 (load WeaponCamo 28 (full)
  (push 5)
  (xstring 0)
  (ifnz 4
   (assetload 4 GfxImage)
  )
  (ifnz 8
   (assetload 8 GfxImage)
  )
  (ifnz 12
   (alloc 12 (n 4) 0
    (arr 12 1 WeaponCamoSet 1 (v WeaponCamo (m 16 0) (t 4 u)))
   )
  )
  (ifnz 20
   (alloc 20 (n 4) 0
    (arr 20 1 WeaponCamoMaterialSet 1 (v WeaponCamo (m 24 0) (t 4 u)))
   )
  )
  (pop)
 )
 (loadptr WeaponCamo 1 (n 4))
)
(asset WeaponVariantDef
 (ptrarray XModel 248 XModel 1 1 1 (n 4))
 (ptrarray WeaponAttachment 284 WeaponAttachment 1 1 1 (n 4))
 (ptrarray WeaponAttachmentUnique 424 WeaponAttachmentUnique 1 1 1 (n 4))
 (load WeaponDef 2448 (full)
  (xstring 0)
  (ifnz 4
   (reuse 4 0
    (alloc 4 (n 4) 0
     (ptrarr 4 1 XModel 1 (n 16))
    )
   )
  )
  (ifnz 8
   (assetload 8 XModel)
  )
  (xstring 12)
  (ifnz 16
   (reuse 16 0
    (alloc 16 (n 2) 0
     (raw 16 1 2 (n 20))
    )
   )
  )
  (ifnz 20
   (reuse 20 0
    (alloc 20 (n 2) 0
     (raw 20 1 2 (n 20))
    )
   )
  )
  (xstring 64)
  (ifnz 108
   (assetload 108 FxEffectDef)
  )
  (ifnz 112
   (assetload 112 FxEffectDef)
  )
  (ifnz 116
   (assetload 116 FxEffectDef)
  )
  (xstring 148)
  (xstring 152)
  (xstring 156)
  (xstring 160)
  (xstring 164)
  (xstring 168)
  (xstring 172)
  (xstring 176)
  (xstring 180)
  (xstring 184)
  (xstring 188)
  (xstring 192)
  (xstring 196)
  (xstring 200)
  (xstring 204)
  (xstring 208)
  (xstring 212)
  (xstring 216)
  (xstring 220)
  (xstring 224)
  (xstring 228)
  (xstring 232)
  (xstring 236)
  (xstring 240)
  (xstring 244)
  (xstring 248)
  (xstring 252)
  (xstring 256)
  (xstring 260)
  (xstring 264)
  (xstring 268)
  (xstring 272)
  (xstring 276)
  (xstring 280)
  (xstring 284)
  (xstring 288)
  (xstring 292)
  (xstring 296)
  (xstring 300)
  (xstring 304)
  (xstring 308)
  (xstring 312)
  (xstring 316)
  (xstring 320)
  (xstring 324)
  (xstring 328)
  (xstring 332)
  (xstring 336)
  (xstring 340)
  (xstring 344)
  (xstring 348)
  (xstring 352)
  (xstring 356)
  (xstring 360)
  (xstring 364)
  (xstring 368)
  (xstring 372)
  (xstring 376)
  (xstring 380)
  (xstring 384)
  (xstring 388)
  (xstring 392)
  (xstring 396)
  (xstring 400)
  (xstring 404)
  (xstring 408)
  (xstring 412)
  (xstring 416)
  (xstring 420)
  (xstring 424)
  (xstring 428)
  (xstring 432)
  (ifnz 436
   (reuse 436 0
    (alloc 436 (n 4) 0
     (xstrarr 436 1 1 (n 32))
    )
   )
  )
  (xstring 440)
  (xstring 444)
  (xstring 448)
  (ifnz 464
   (assetload 464 FxEffectDef)
  )
  (ifnz 468
   (assetload 468 FxEffectDef)
  )
  (ifnz 472
   (assetload 472 FxEffectDef)
  )
  (ifnz 476
   (assetload 476 FxEffectDef)
  )
  (ifnz 528
   (assetload 528 Material)
  )
  (ifnz 532
   (assetload 532 Material)
  )
  (ifnz 916
   (reuse 916 0
    (alloc 916 (n 4) 0
     (ptrarr 916 1 XModel 1 (n 16))
    )
   )
  )
  (ifnz 920
   (assetload 920 XModel)
  )
  (ifnz 924
   (assetload 924 XModel)
  )
  (ifnz 928
   (assetload 928 XModel)
  )
  (ifnz 932
   (assetload 932 XModel)
  )
  (ifnz 936
   (assetload 936 Material)
  )
  (ifnz 940
   (assetload 940 Material)
  )
  (ifnz 956
   (assetload 956 Material)
  )
  (xstring 980)
  (xstring 1100)
  (xstring 1104)
  (xstring 1108)
  (xstring 1112)
  (xstring 1116)
  (xstring 1120)
  (xstring 1388)
  (ifnz 1632
   (assetload 1632 Material)
  )
  (ifnz 948
   (assetload 948 Material)
  )
  (xstring 1652)
  (xstring 1656)
  (ifnz 1768
   (assetload 1768 XModel)
  )
  (ifnz 1776
   (assetload 1776 FxEffectDef)
  )
  (ifnz 1784
   (assetload 1784 FxEffectDef)
  )
  (ifnz 1792
   (assetload 1792 FxEffectDef)
  )
  (ifnz 1800
   (assetload 1800 FxEffectDef)
  )
  (ifnz 1808
   (assetload 1808 FxEffectDef)
  )
  (ifnz 1816
   (assetload 1816 FxEffectDef)
  )
  (xstring 1820)
  (xstring 1824)
  (xstring 1828)
  (xstring 1832)
  (ifnz 1880
   (reuse 1880 0
    (alloc 1880 (n 4) 0
     (raw 1880 1 4 (n 32))
    )
   )
  )
  (ifnz 1884
   (reuse 1884 0
    (alloc 1884 (n 4) 0
     (raw 1884 1 4 (n 32))
    )
   )
  )
  (ifnz 1888
   (assetload 1888 FxEffectDef)
  )
  (ifnz 1916
   (assetload 1916 FxEffectDef)
  )
  (xstring 1920)
  (xstring 2124)
  (ifnz 2132
   (reuse 2132 0
    (alloc 2132 (n 4) 0
     (raw 2132 1 8 (v WeaponDef (m 2148 0) (t 4 i)))
    )
   )
  )
  (ifnz 2140
   (reuse 2140 0
    (alloc 2140 (n 4) 0
     (raw 2140 1 8 (v WeaponDef (m 2148 0) (t 4 i)))
    )
   )
  )
  (xstring 2128)
  (ifnz 2136
   (reuse 2136 0
    (alloc 2136 (n 4) 0
     (raw 2136 1 8 (v WeaponDef (m 2152 0) (t 4 i)))
    )
   )
  )
  (ifnz 2144
   (reuse 2144 0
    (alloc 2144 (n 4) 0
     (raw 2144 1 8 (v WeaponDef (m 2152 0) (t 4 i)))
    )
   )
  )
  (xstring 2236)
  (xstring 2240)
  (xstring 2284)
  (ifnz 2300
   (reuse 2300 0
    (alloc 2300 (n 4) 0
     (raw 2300 1 4 (n 21))
    )
   )
  )
  (xstring 2304)
  (xstring 2308)
  (xstring 2312)
  (xstring 2316)
  (ifnz 2320
   (assetload 2320 TracerDef)
  )
  (ifnz 2324
   (assetload 2324 TracerDef)
  )
  (xstring 2356)
  (xstring 2360)
  (ifnz 2364
   (reuse 2364 0
    (alloc 2364 (n 4) 0
     (single 2364 FlameTable)
    )
   )
  )
  (ifnz 2368
   (reuse 2368 0
    (alloc 2368 (n 4) 0
     (single 2368 FlameTable)
    )
   )
  )
  (ifnz 2372
   (assetload 2372 FxEffectDef)
  )
  (ifnz 2376
   (assetload 2376 FxEffectDef)
  )
  (ifnz 2404
   (assetload 2404 FxEffectDef)
  )
  (ifnz 2408
   (assetload 2408 FxEffectDef)
  )
  (ifnz 2412
   (assetload 2412 FxEffectDef)
  )
  (xstring 2416)
  (ifnz 2420
   (assetload 2420 WeaponCamo)
  )
 )
 (load FlameTable 484 (full)
  (xstring 432)
  (ifnz 436
   (assetload 436 Material)
  )
  (ifnz 440
   (assetload 440 Material)
  )
  (ifnz 444
   (assetload 444 Material)
  )
  (ifnz 448
   (assetload 448 Material)
  )
  (ifnz 452
   (assetload 452 Material)
  )
  (ifnz 456
   (assetload 456 Material)
  )
  (ifnz 460
   (assetload 460 Material)
  )
  (ifnz 464
   (assetload 464 Material)
  )
  (xstring 468)
  (xstring 472)
  (xstring 476)
  (xstring 480)
 )
 (load WeaponVariantDef 716 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (reuse 8 0
    (alloc 8 (n 4) 0
     (single 8 WeaponDef)
    )
   )
  )
  (xstring 12)
  (xstring 16)
  (xstring 20)
  (ifnz 24
   (reuse 24 0
    (alloc 24 (n 4) 0
     (ptrarr 24 1 WeaponAttachment 1 (n 63))
    )
   )
  )
  (ifnz 28
   (reuse 28 0
    (alloc 28 (n 4) 0
     (ptrarr 28 1 WeaponAttachmentUnique 1 (n 95))
    )
   )
  )
  (ifnz 32
   (reuse 32 0
    (alloc 32 (n 4) 0
     (xstrarr 32 1 1 (n 88))
    )
   )
  )
  (ifnz 36
   (reuse 36 0
    (alloc 36 (n 2) 0
     (raw 36 1 2 (n 32))
    )
   )
  )
  (ifnz 40
   (reuse 40 0
    (alloc 40 (n 4) 0
     (ptrarr 40 1 XModel 1 (n 8))
    )
   )
  )
  (ifnz 44
   (reuse 44 0
    (alloc 44 (n 4) 0
     (ptrarr 44 1 XModel 1 (n 8))
    )
   )
  )
  (ifnz 48
   (reuse 48 0
    (alloc 48 (n 4) 0
     (xstrarr 48 1 1 (n 8))
    )
   )
  )
  (ifnz 52
   (reuse 52 0
    (alloc 52 (n 4) 0
     (xstrarr 52 1 1 (n 8))
    )
   )
  )
  (xstring 508)
  (xstring 512)
  (xstring 520)
  (ifnz 596
   (assetload 596 Material)
  )
  (ifnz 600
   (assetload 600 Material)
  )
  (ifnz 604
   (assetload 604 Material)
  )
  (pop)
 )
 (loadptr WeaponVariantDef 1 (n 4))
)
(asset XAnimParts
 (array XAnimNotifyInfo 8 XAnimNotifyInfo)
 (load XAnimNotifyInfo 8 (full)
 )
 (load XAnimDeltaPart 12 (full)
  (ifnz 0
   (alloc 0 (n 4) 0
    (single 0 XAnimPartTrans)
   )
  )
  (ifnz 4
   (alloc 4 (n 4) 0
    (single 4 XAnimDeltaPartQuat2)
   )
  )
  (ifnz 8
   (alloc 8 (n 4) 0
    (single 8 XAnimDeltaPartQuat)
   )
  )
 )
 (load XAnimPartTrans 36 (partial 4)
  (embedded 4 XAnimPartTransData 1)
 )
 (load XAnimPartTransData 32 (none)
  (chain
   (case (op > (v XAnimPartTrans (m 0 0) (t 2 u)) (n 0))
    (embedded 0 XAnimPartTransFrames 1)
   )
   (case (else)
    (raw 0 0 12 (n 1))
   )
  )
 )
 (load XAnimPartTransFrames 32 (partial 28)
  (embedded 28 XAnimDynamicIndicesTrans 1)
  (embedded 24 XAnimDynamicFrames 0)
 )
 (load XAnimDynamicIndicesTrans 2 (none)
  (chain
   (case (op < (v XAnimParts (m 14 0) (t 2 u)) (n 256))
    (raw 0 0 1 (op + (v XAnimPartTrans (m 0 0) (t 2 u)) (n 1)))
   )
   (case (else)
    (raw 0 0 2 (op + (v XAnimPartTrans (m 0 0) (t 2 u)) (n 1)))
   )
  )
 )
 (load XAnimDynamicFrames 4 (full)
  (chain
   (case (v XAnimPartTrans (m 2 0) (t 1 u))
    (ifnz 0
     (alloc 0 (n 1) 0
      (raw 0 1 3 (op + (v XAnimPartTrans (m 0 0) (t 2 u)) (n 1)))
     )
    )
   )
   (case (else)
    (ifnz 0
     (alloc 0 (n 4) 0
      (raw 0 1 6 (op + (v XAnimPartTrans (m 0 0) (t 2 u)) (n 1)))
     )
    )
   )
  )
 )
 (load XAnimDeltaPartQuat2 12 (partial 4)
  (embedded 4 XAnimDeltaPartQuatData2 1)
 )
 (load XAnimDeltaPartQuatData2 8 (none)
  (chain
   (case (op > (v XAnimDeltaPartQuat2 (m 0 0) (t 2 u)) (n 0))
    (embedded 0 XAnimDeltaPartQuatDataFrames2 1)
   )
   (case (else)
    (raw 0 0 4 (n 1))
   )
  )
 )
 (load XAnimDeltaPartQuatDataFrames2 8 (partial 4)
  (embedded 4 XAnimDynamicIndicesQuat2 1)
  (ifnz 0
   (alloc 0 (n 4) 0
    (raw 0 1 4 (op + (v XAnimDeltaPartQuat2 (m 0 0) (t 2 u)) (n 1)))
   )
  )
 )
 (load XAnimDynamicIndicesQuat2 2 (none)
  (chain
   (case (op < (v XAnimParts (m 14 0) (t 2 u)) (n 256))
    (raw 0 0 1 (op + (v XAnimDeltaPartQuat2 (m 0 0) (t 2 u)) (n 1)))
   )
   (case (else)
    (raw 0 0 2 (op + (v XAnimDeltaPartQuat2 (m 0 0) (t 2 u)) (n 1)))
   )
  )
 )
 (load XAnimDeltaPartQuat 12 (partial 4)
  (embedded 4 XAnimDeltaPartQuatData 1)
 )
 (load XAnimDeltaPartQuatData 8 (none)
  (chain
   (case (op > (v XAnimDeltaPartQuat (m 0 0) (t 2 u)) (n 0))
    (embedded 0 XAnimDeltaPartQuatDataFrames 1)
   )
   (case (else)
    (raw 0 0 8 (n 1))
   )
  )
 )
 (load XAnimDeltaPartQuatDataFrames 8 (partial 4)
  (embedded 4 XAnimDynamicIndicesQuat 1)
  (ifnz 0
   (alloc 0 (n 4) 0
    (raw 0 1 8 (op + (v XAnimDeltaPartQuat (m 0 0) (t 2 u)) (n 1)))
   )
  )
 )
 (load XAnimDynamicIndicesQuat 2 (none)
  (chain
   (case (op < (v XAnimParts (m 14 0) (t 2 u)) (n 256))
    (raw 0 0 1 (op + (v XAnimDeltaPartQuat (m 0 0) (t 2 u)) (n 1)))
   )
   (case (else)
    (raw 0 0 2 (op + (v XAnimDeltaPartQuat (m 0 0) (t 2 u)) (n 1)))
   )
  )
 )
 (load XAnimIndices 4 (full)
  (chain
   (case (op < (v XAnimParts (m 14 0) (t 2 u)) (n 256))
    (ifnz 0
     (alloc 0 (n 1) 0
      (raw 0 1 1 (v XAnimParts (m 44 0) (t 4 u)))
     )
    )
   )
   (case (else)
    (ifnz 0
     (alloc 0 (n 2) 0
      (raw 0 1 2 (v XAnimParts (m 44 0) (t 4 u)))
     )
    )
   )
  )
 )
 (load XAnimParts 104 (full)
  (push 5)
  (xstring 0)
  (ifnz 64
   (alloc 64 (n 2) 0
    (raw 64 1 2 (v XAnimParts (m 24 0) (i (n 9) 1) (t 1 u)))
   )
  )
  (ifnz 96
   (alloc 96 (n 4) 0
    (arr 96 1 XAnimNotifyInfo 1 (v XAnimParts (m 34 0) (t 1 u)))
   )
  )
  (ifnz 100
   (alloc 100 (n 4) 0
    (single 100 XAnimDeltaPart)
   )
  )
  (ifnz 68
   (alloc 68 (n 1) 0
    (raw 68 1 1 (v XAnimParts (m 4 0) (t 2 u)))
   )
  )
  (ifnz 72
   (alloc 72 (n 2) 0
    (raw 72 1 2 (v XAnimParts (m 6 0) (t 2 u)))
   )
  )
  (ifnz 76
   (alloc 76 (n 4) 0
    (raw 76 1 4 (v XAnimParts (m 8 0) (t 2 u)))
   )
  )
  (ifnz 80
   (alloc 80 (n 2) 0
    (raw 80 1 2 (v XAnimParts (m 40 0) (t 4 u)))
   )
  )
  (ifnz 84
   (alloc 84 (n 1) 0
    (raw 84 1 1 (v XAnimParts (m 10 0) (t 2 u)))
   )
  )
  (ifnz 88
   (alloc 88 (n 4) 0
    (raw 88 1 4 (v XAnimParts (m 12 0) (t 2 u)))
   )
  )
  (embedded 92 XAnimIndices 0)
  (pop)
 )
 (loadptr XAnimParts 1 (n 4))
)
(asset XGlobals
 (array gump_info_t 8 gump_info_t)
 (array overlay_info_t 8 overlay_info_t)
 (load gump_info_t 8 (full)
  (xstring 0)
 )
 (load overlay_info_t 8 (full)
  (xstring 0)
 )
 (load XGlobals 564 (full)
  (push 5)
  (xstring 0)
  (if (op && (op >= (v XGlobals (m 40 0) (t 4 i)) (n 0)) (op <= (v XGlobals (m 40 0) (t 4 i)) (n 32)))
   (arr 44 0 gump_info_t 0 (v XGlobals (m 40 0) (t 4 i)))
  )
  (if (op && (op >= (v XGlobals (m 304 0) (t 4 i)) (n 0)) (op <= (v XGlobals (m 304 0) (t 4 i)) (n 32)))
   (arr 308 0 overlay_info_t 0 (v XGlobals (m 304 0) (t 4 i)))
  )
  (pop)
 )
 (loadptr XGlobals 1 (n 4))
)
(asset XModel
 (ptrarray Material 112 Material 0 1 1 (n 4))
 (array XSurface 80 XSurface)
 (array XRigidVertList 12 XRigidVertList)
 (array XModelCollSurf_s 44 XModelCollSurf_s)
 (array Collmap 4 Collmap)
 (array PhysGeomInfo16 68 PhysGeomInfo)
 (array cbrushside_t 12 cbrushside_t)
 (load XSurface 80 (full)
  (embedded 16 XSurfaceVertexInfo 0)
  (if (op == (op & (v XSurface (m 2 0) (t 2 u)) (n 1)) (n 0))
   (ifnz 32
    (reuse 32 0
     (alloc 32 (n 16) 0
      (raw 32 1 32 (v XSurface (m 4 0) (t 2 u)))
     )
    )
   )
  )
  (ifnz 40
   (reuse 40 0
    (alloc 40 (n 4) 0
     (arr 40 1 XRigidVertList 1 (v XSurface (m 1 0) (t 1 u)))
    )
   )
  )
  (ifnz 12
   (reuse 12 0
    (alloc 12 (n 16) 0
     (raw 12 1 6 (v XSurface (m 6 0) (t 2 u)))
    )
   )
  )
 )
 (load XSurfaceVertexInfo 16 (full)
  (ifnz 8
   (reuse 8 0
    (alloc 8 (n 2) 0
     (raw 8 1 2 (op + (op + (op + (v XSurfaceVertexInfo (m 0 0) (i (n 0) 2) (t 2 i)) (op * (n 3) (v XSurfaceVertexInfo (m 0 0) (i (n 1) 2) (t 2 i)))) (op * (n 5) (v XSurfaceVertexInfo (m 0 0) (i (n 2) 2) (t 2 i)))) (op * (n 7) (v XSurfaceVertexInfo (m 0 0) (i (n 3) 2) (t 2 i)))))
    )
   )
  )
  (ifnz 12
   (reuse 12 0
    (alloc 12 (n 4) 0
     (raw 12 1 4 (op + (op + (op + (v XSurfaceVertexInfo (m 0 0) (i (n 0) 2) (t 2 i)) (v XSurfaceVertexInfo (m 0 0) (i (n 1) 2) (t 2 i))) (v XSurfaceVertexInfo (m 0 0) (i (n 2) 2) (t 2 i))) (v XSurfaceVertexInfo (m 0 0) (i (n 3) 2) (t 2 i))))
    )
   )
  )
 )
 (load XRigidVertList 12 (full)
  (ifnz 8
   (reuse 8 0
    (alloc 8 (n 4) 0
     (single 8 XSurfaceCollisionTree)
    )
   )
  )
 )
 (load XSurfaceCollisionTree 40 (full)
  (ifnz 28
   (alloc 28 (n 16) 0
    (raw 28 1 16 (v XSurfaceCollisionTree (m 24 0) (t 4 u)))
   )
  )
  (ifnz 36
   (alloc 36 (n 2) 0
    (raw 36 1 2 (v XSurfaceCollisionTree (m 32 0) (t 4 u)))
   )
  )
 )
 (load XModelCollSurf_s 44 (full)
  (ifnz 0
   (alloc 0 (n 4) 0
    (raw 0 1 48 (v XModelCollSurf_s (m 4 0) (t 4 i)))
   )
  )
 )
 (load Collmap 4 (full)
  (ifnz 0
   (alloc 0 (n 4) 0
    (single 0 PhysGeomList)
   )
  )
 )
 (load PhysGeomList 12 (full)
  (ifnz 4
   (alloc 4 (n 16) 0
    (arr 4 1 PhysGeomInfo16 1 (v PhysGeomList (m 0 0) (t 4 u)))
   )
  )
 )
 (load PhysGeomInfo 68 (full)
  (ifnz 0
   (reuse 0 0
    (alloc 0 (n 16) 0
     (single 0 BrushWrapper)
    )
   )
  )
 )
 (load BrushWrapper 96 (full)
  (ifnz 32
   (alloc 32 (n 4) 0
    (arr 32 1 cbrushside_t 1 (v BrushWrapper (m 28 0) (t 4 u)))
   )
  )
  (ifnz 88
   (reuse 88 0
    (alloc 88 (n 4) 0
     (raw 88 1 12 (v BrushWrapper (m 84 0) (t 4 u)))
    )
   )
  )
  (ifnz 92
   (reuse 92 0
    (alloc 92 (n 4) 0
     (raw 92 1 20 (v BrushWrapper (m 28 0) (t 4 u)))
    )
   )
  )
 )
 (load cbrushside_t 12 (full)
  (ifnz 0
   (reuse 0 0
    (alloc 0 (n 4) 0
     (raw 0 1 20 (n 1))
    )
   )
  )
 )
 (load XModel 248 (full)
  (push 5)
  (xstring 0)
  (ifnz 8
   (reuse 8 0
    (alloc 8 (n 2) 0
     (raw 8 1 2 (v XModel (m 4 0) (t 1 u)))
    )
   )
  )
  (ifnz 12
   (reuse 12 0
    (alloc 12 (n 1) 0
     (raw 12 1 1 (op - (v XModel (m 4 0) (t 1 u)) (v XModel (m 5 0) (t 1 u))))
    )
   )
  )
  (ifnz 16
   (reuse 16 0
    (alloc 16 (n 2) 0
     (raw 16 1 8 (op - (v XModel (m 4 0) (t 1 u)) (v XModel (m 5 0) (t 1 u))))
    )
   )
  )
  (ifnz 20
   (reuse 20 0
    (alloc 20 (n 4) 0
     (raw 20 1 4 (op * (op - (v XModel (m 4 0) (t 1 u)) (v XModel (m 5 0) (t 1 u))) (n 4)))
    )
   )
  )
  (ifnz 24
   (reuse 24 0
    (alloc 24 (n 1) 0
     (raw 24 1 1 (v XModel (m 4 0) (t 1 u)))
    )
   )
  )
  (ifnz 28
   (reuse 28 0
    (alloc 28 (n 4) 0
     (raw 28 1 32 (v XModel (m 4 0) (t 1 u)))
    )
   )
  )
  (ifnz 32
   (alloc 32 (n 16) 0
    (arr 32 1 XSurface 1 (v XModel (m 6 0) (t 1 u)))
   )
  )
  (ifnz 36
   (alloc 36 (n 4) 0
    (ptrarr 36 1 Material 1 (v XModel (m 6 0) (t 1 u)))
   )
  )
  (ifnz 152
   (alloc 152 (n 4) 0
    (arr 152 1 XModelCollSurf_s 1 (v XModel (m 156 0) (t 4 i)))
   )
  )
  (ifnz 164
   (alloc 164 (n 4) 0
    (raw 164 1 44 (v XModel (m 4 0) (t 1 u)))
   )
  )
  (ifnz 200
   (alloc 200 (n 4) 0
    (raw 200 1 4 (v XModel (m 6 0) (t 1 u)))
   )
  )
  (ifnz 216
   (assetload 216 PhysPreset)
  )
  (ifnz 224
   (alloc 224 (n 4) 0
    (arr 224 1 Collmap 1 (v XModel (m 220 0) (t 1 u)))
   )
  )
  (ifnz 228
   (assetload 228 PhysConstraints)
  )
  (pop)
 )
 (loadptr XModel 1 (n 4))
)
(asset ZBarrierDef
 (array ZBarrierBoard 80 ZBarrierBoard)
 (load ZBarrierBoard 80 (full)
  (ifnz 0
   (assetload 0 XModel)
  )
  (ifnz 4
   (assetload 4 XModel)
  )
  (ifnz 8
   (assetload 8 XModel)
  )
  (xstring 12)
  (xstring 16)
  (ifnz 20
   (assetload 20 FxEffectDef)
  )
  (ifnz 24
   (assetload 24 FxEffectDef)
  )
 )
 (load ZBarrierDef 560 (full)
  (push 5)
  (xstring 0)
  (arr 80 0 ZBarrierBoard 0 (n 6))
  (pop)
 )
 (loadptr ZBarrierDef 1 (n 4))
)
