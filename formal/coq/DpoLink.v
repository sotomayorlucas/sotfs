(* ===================================================================== *)
(* DpoLink.v — DPO rule LINK (hard link) + invariant preservation proofs *)
(*                                                                       *)
(* Corresponds to:                                                       *)
(*   TLA+:  Link(d, name, target_ino) in sotfs_graph.tla                *)
(*   Rust:  link() in sotfs-ops/src/lib.rs                               *)
(*                                                                       *)
(* The rule adds a new Contains edge (d, target_ino, name) and           *)
(* increments the target inode's link_count by 1.                        *)
(* No new nodes are created — hard links share the same inode.           *)
(*                                                                       *)
(* Preconditions (gluing conditions):                                    *)
(*   GC-LINK-1: no existing entry with this name in d                    *)
(*   GC-LINK-2: target is a Regular inode (cannot hard-link directories) *)
(*   GC-LINK-3: link_count < LINK_MAX (not modeled — nat is unbounded)  *)
(* ===================================================================== *)

Require Import Coq.Arith.Arith.
Require Import Coq.Lists.List.
Require Import Coq.Bool.Bool.
Require Import Lia.
Import ListNotations.

Require Import SotfsGraph.

(* ===================================================================== *)
(* 1. Preconditions                                                      *)
(* ===================================================================== *)

Record LinkPre (g : Graph) (d : DirId) (name : Name)
  (target_ino : InodeId) : Prop := {
  lp_dir_exists    : dir_exists g d;
  lp_user_name     : is_user_name name;
  lp_name_fresh    : name_in_dir g d name = false;
  lp_target_exists : inode_exists g target_ino;
  lp_is_regular    : forall ir, find_inode g target_ino = Some ir ->
                        ir_vtype ir = Regular;
}.

(* ===================================================================== *)
(* 2. Helper: increment link_count of one inode                          *)
(* ===================================================================== *)

Definition increment_link (inodes : list InodeRec) (ino : InodeId)
  : list InodeRec :=
  map (fun ir =>
    if Nat.eqb (ir_id ir) ino
    then mkInode (ir_id ir) (ir_vtype ir) (S (ir_link_count ir))
    else ir
  ) inodes.

Lemma increment_link_preserves_ids :
  forall inodes ino,
    map ir_id (increment_link inodes ino) = map ir_id inodes.
Proof.
  intros. unfold increment_link.
  rewrite map_map. apply map_ext.
  intro a. simpl. destruct (Nat.eqb (ir_id a) ino); reflexivity.
Qed.

Lemma increment_link_In :
  forall inodes ino ir,
    In ir inodes ->
    ir_id ir <> ino ->
    In ir (increment_link inodes ino).
Proof.
  intros inodes ino ir Hin Hneq.
  unfold increment_link. apply in_map_iff.
  exists ir. split.
  - destruct (Nat.eqb (ir_id ir) ino) eqn:Heq.
    + apply Nat.eqb_eq in Heq. contradiction.
    + reflexivity.
  - exact Hin.
Qed.

(* find through increment_link preserves vtype *)
Lemma increment_link_find_vtype :
  forall inodes ino target_ino ir,
    find (fun x => Nat.eqb (ir_id x) ino)
         (map (fun x =>
           if Nat.eqb (ir_id x) target_ino
           then mkInode (ir_id x) (ir_vtype x) (S (ir_link_count x))
           else x) inodes) = Some ir ->
    exists ir_old,
      find (fun x => Nat.eqb (ir_id x) ino) inodes = Some ir_old /\
      ir_vtype ir = ir_vtype ir_old.
Proof.
  intros inodes ino target_ino.
  induction inodes as [|h t IH]; intros ir Hfind.
  - simpl in Hfind. discriminate.
  - simpl in Hfind.
    destruct (Nat.eqb (ir_id h) target_ino) eqn:Htgt.
    + simpl in Hfind.
      destruct (Nat.eqb (ir_id h) ino) eqn:Hid.
      * inversion Hfind. subst. exists h. simpl. rewrite Hid. split; reflexivity.
      * apply IH in Hfind. destruct Hfind as [ir_old [Hf Hv]].
        exists ir_old. simpl. rewrite Hid. exact (conj Hf Hv).
    + simpl in Hfind.
      destruct (Nat.eqb (ir_id h) ino) eqn:Hid.
      * inversion Hfind as [Heq]. subst ir.
        exists h. simpl. rewrite Hid. split; reflexivity.
      * apply IH in Hfind. destruct Hfind as [ir_old [Hf Hv]].
        exists ir_old. simpl. rewrite Hid. exact (conj Hf Hv).
Qed.

(* ===================================================================== *)
(* 3. The link function                                                  *)
(* ===================================================================== *)

Definition hard_link (g : Graph) (d : DirId) (name : Name)
  (target_ino : InodeId) : Graph :=
  {| g_inodes := increment_link (g_inodes g) target_ino;
     g_dirs   := g_dirs g;
     g_edges  := g_edges g ++ [mkContains d target_ino name];
  |}.

(* ===================================================================== *)
(* 4. Auxiliary lemmas                                                   *)
(* ===================================================================== *)

Lemma link_edges :
  forall g d name ti e,
    In e (g_edges (hard_link g d name ti)) <->
    In e (g_edges g) \/ e = mkContains d ti name.
Proof.
  intros. unfold hard_link. simpl. rewrite in_app_iff. simpl.
  (* Coq 8.20: tauto doesn't symmetrize `=`; do it manually. *)
  split.
  - intros [Hold | [Hnew | []]]; [left; assumption | right; symmetry; assumption].
  - intros [Hold | Hnew]; [left; assumption | right; left; symmetry; assumption].
Qed.

Lemma link_preserves_dirs :
  forall g d name ti id,
    dir_exists g id <-> dir_exists (hard_link g d name ti) id.
Proof.
  intros. unfold dir_exists, dir_ids, hard_link. simpl. tauto.
Qed.

Lemma link_preserves_inodes :
  forall g d name ti id,
    inode_exists g id -> inode_exists (hard_link g d name ti) id.
Proof.
  intros. unfold inode_exists, inode_ids, hard_link in *. simpl.
  rewrite increment_link_preserves_ids. exact H.
Qed.

(* incoming_count for non-target inodes is unchanged *)
Lemma link_incoming_other :
  forall g d name ti ino,
    ino <> ti ->
    incoming_count (hard_link g d name ti) ino =
    incoming_count g ino.
Proof.
  intros. unfold incoming_count, hard_link. simpl.
  rewrite count_occ_pred_app. simpl.
  assert (H1 : Nat.eqb ti ino = false) by (apply Nat.eqb_neq; auto).
  rewrite H1. simpl. lia.
Qed.

(* incoming_count for target = old + 1 *)
Lemma link_incoming_target :
  forall g d name ti,
    is_user_name name ->
    incoming_count (hard_link g d name ti) ti =
    S (incoming_count g ti).
Proof.
  intros g d name ti Huser.
  unfold incoming_count, hard_link. simpl.
  rewrite count_occ_pred_app. simpl.
  rewrite Nat.eqb_refl.
  assert (Hndot : Nat.eqb name dotdot_name = false).
  { apply Nat.eqb_neq. apply user_name_not_dotdot. exact Huser. }
  rewrite Hndot. simpl. lia.
Qed.

(* find_inode for target gives incremented version *)
Lemma link_find_target :
  forall g d name ti ir_old,
    find_inode g ti = Some ir_old ->
    find_inode (hard_link g d name ti) ti =
      Some (mkInode (ir_id ir_old) (ir_vtype ir_old) (S (ir_link_count ir_old))).
Proof.
  intros g d name ti ir_old Hfind.
  unfold find_inode, hard_link. simpl. unfold increment_link.
  unfold find_inode in Hfind.
  induction (g_inodes g) as [|h t IH].
  - simpl in Hfind. discriminate.
  - simpl in Hfind. simpl.
    destruct (Nat.eqb (ir_id h) ti) eqn:Htgt.
    + simpl. rewrite Htgt. inversion Hfind. subst. reflexivity.
    + simpl. rewrite Htgt. apply IH. exact Hfind.
Qed.

(* find_inode for non-target is unchanged *)
Lemma link_find_other :
  forall g d name ti ino,
    ino <> ti ->
    find_inode (hard_link g d name ti) ino = find_inode g ino.
Proof.
  intros g d name ti ino Hneq.
  unfold find_inode, hard_link. simpl. unfold increment_link.
  induction (g_inodes g) as [|h t IH].
  - simpl. reflexivity.
  - simpl.
    destruct (Nat.eqb (ir_id h) ti) eqn:Htgt.
    + simpl. destruct (Nat.eqb (ir_id h) ino) eqn:Hid.
      * apply Nat.eqb_eq in Htgt. apply Nat.eqb_eq in Hid.
        subst. contradiction.
      * apply IH.
    + simpl. destruct (Nat.eqb (ir_id h) ino) eqn:Hid.
      * reflexivity.
      * apply IH.
Qed.

(* ===================================================================== *)
(* 5. THEOREM: hard_link preserves TypeInvariant                         *)
(* ===================================================================== *)

Theorem link_preserves_TypeInvariant :
  forall g d name ti,
    WellFormed g ->
    LinkPre g d name ti ->
    TypeInvariant (hard_link g d name ti).
Proof.
  intros g d name ti HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct HTI as [Hedge_endpts [HnodupI HnodupD]].
  destruct Hpre as [Hdir Huser Hfresh Htgt Hreg].
  unfold TypeInvariant. split; [| split].
  - intros e0 Hin. apply link_edges in Hin.
    destruct Hin as [Hold | Hnew].
    + destruct (HND e0 Hold) as [Hd Hi]. split.
      * apply link_preserves_dirs. exact Hd.
      * apply link_preserves_inodes. exact Hi.
    + subst e0. simpl. split.
      * apply link_preserves_dirs. exact Hdir.
      * apply link_preserves_inodes. exact Htgt.
  - unfold NoDupInodeIds, inode_ids, hard_link. simpl.
    rewrite increment_link_preserves_ids. exact HnodupI.
  - unfold NoDupDirIds, dir_ids, hard_link. simpl. exact HnodupD.
Qed.

(* ===================================================================== *)
(* 6. THEOREM: hard_link preserves LinkCountConsistent                   *)
(* ===================================================================== *)

Theorem link_preserves_LinkCountConsistent :
  forall g d name ti,
    WellFormed g ->
    LinkPre g d name ti ->
    LinkCountConsistent (hard_link g d name ti).
Proof.
  intros g d name ti HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct Hpre as [Hdir Huser Hfresh Htgt Hreg].
  unfold LinkCountConsistent.
  intros ir Hin. unfold hard_link in Hin. simpl in Hin.
  unfold increment_link in Hin. apply in_map_iff in Hin.
  destruct Hin as [ir_old [Heq Hin_old]].
  destruct (Nat.eq_dec (ir_id ir_old) ti) as [Htgt_eq | Hntgt].
  - (* ir_old is the target — link_count incremented *)
    assert (Heqb : Nat.eqb (ir_id ir_old) ti = true).
    { apply Nat.eqb_eq. exact Htgt_eq. }
    rewrite Heqb in Heq. subst ir. simpl.
    rewrite Htgt_eq.
    rewrite (link_incoming_target g d name ti Huser).
    f_equal. rewrite <- Htgt_eq. apply HLC. exact Hin_old.
  - (* ir_old is not the target — unchanged *)
    assert (Heqb : Nat.eqb (ir_id ir_old) ti = false).
    { apply Nat.eqb_neq. exact Hntgt. }
    rewrite Heqb in Heq. subst ir.
    rewrite (link_incoming_other g d name ti (ir_id ir_old) Hntgt).
    apply HLC. exact Hin_old.
Qed.

(* ===================================================================== *)
(* 7. THEOREM: hard_link preserves UniqueNamesPerDir                     *)
(* ===================================================================== *)

Theorem link_preserves_UniqueNamesPerDir :
  forall g d name ti,
    WellFormed g ->
    LinkPre g d name ti ->
    UniqueNamesPerDir (hard_link g d name ti).
Proof.
  intros g d name ti HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct Hpre as [Hdir Huser Hfresh Htgt Hreg].
  unfold UniqueNamesPerDir.
  intros e1 e2 Hin1 Hin2 Hdir_eq Hname_eq.
  apply link_edges in Hin1. apply link_edges in Hin2.
  destruct Hin1 as [H1o | H1n]; destruct Hin2 as [H2o | H2n].
  - apply HUN; assumption.
  - subst e2. simpl in *. exfalso.
    apply (name_in_dir_false_not_in g d name Hfresh).
    exists (ce_ino e1). destruct e1. simpl in *. subst. exact H1o.
  - subst e1. simpl in *. exfalso.
    apply (name_in_dir_false_not_in g d name Hfresh).
    exists (ce_ino e2). destruct e2. simpl in *. subst. exact H2o.
  - subst. reflexivity.
Qed.

(* ===================================================================== *)
(* 8. THEOREM: hard_link preserves NoDanglingEdges                       *)
(* ===================================================================== *)

Theorem link_preserves_NoDanglingEdges :
  forall g d name ti,
    WellFormed g ->
    LinkPre g d name ti ->
    NoDanglingEdges (hard_link g d name ti).
Proof.
  intros g d name ti HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct Hpre as [Hdir Huser Hfresh Htgt Hreg].
  unfold NoDanglingEdges.
  intros e Hin. apply link_edges in Hin.
  destruct Hin as [Hold | Hnew].
  - destruct (HND e Hold) as [Hd Hi]. split.
    + apply link_preserves_dirs. exact Hd.
    + apply link_preserves_inodes. exact Hi.
  - subst e. simpl. split.
    + apply link_preserves_dirs. exact Hdir.
    + apply link_preserves_inodes. exact Htgt.
Qed.

(* ===================================================================== *)
(* 9. THEOREM: hard_link preserves NoDirCycles                           *)
(* ===================================================================== *)

(* Key insight: the target is Regular (not DirectoryType), so the new
   edge cannot create a directory cycle. The old ranking works. *)

Theorem link_preserves_NoDirCycles :
  forall g d name ti,
    WellFormed g ->
    LinkPre g d name ti ->
    NoDirCycles (hard_link g d name ti).
Proof.
  intros g d name ti HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct HTI as [_ [HnodupI _]].
  destruct HNC as [rank Hrank].
  destruct Hpre as [Hdir Huser Hfresh Htgt Hreg].
  exists rank.
  intros e Hin Huser_name ir Hfind Hvtype child_dir Hchild.
  apply link_edges in Hin. destruct Hin as [Hold | Hnew].
  - (* Old edge — use old ranking, find through increment_link *)
    unfold find_inode in Hfind. unfold hard_link in Hfind. simpl in Hfind.
    unfold increment_link in Hfind.
    apply increment_link_find_vtype in Hfind.
    destruct Hfind as [ir_old [Hf Hv]].
    unfold dir_for_inode in Hchild. unfold hard_link in Hchild. simpl in Hchild.
    apply (Hrank e Hold Huser_name ir_old).
    + unfold find_inode. exact Hf.
    + rewrite <- Hv. exact Hvtype.
    + unfold dir_for_inode. exact Hchild.
  - (* New edge: (d, ti, name) — target is Regular *)
    subst e. simpl in *.
    unfold find_inode in Hfind. unfold hard_link in Hfind. simpl in Hfind.
    unfold increment_link in Hfind.
    apply increment_link_find_vtype in Hfind.
    destruct Hfind as [ir_old [Hf Hv]].
    specialize (Hreg ir_old Hf).
    rewrite Hv in Hvtype. rewrite Hreg in Hvtype. discriminate.
Qed.

(* ===================================================================== *)
(* 10. MAIN THEOREM: hard_link preserves WellFormed                      *)
(* ===================================================================== *)

(* hard_link adds a Contains edge to an existing inode and increments
   its link_count. g_dirs is unchanged; the new edge is a user-name
   edge (precondition lp_user_name), so DirHasSelfRef survives. *)
Theorem link_preserves_DirHasSelfRef :
  forall g d name ti,
    WellFormed g ->
    DirHasSelfRef (hard_link g d name ti).
Proof.
  intros g d name ti HWF.
  destruct HWF as [_ [_ [_ [_ [_ [HDSR _]]]]]].
  unfold DirHasSelfRef in *.
  intros d0 Hin. unfold hard_link in *. simpl in *.
  apply in_or_app. left. apply HDSR. exact Hin.
Qed.

(* The new edge (d, ti, name) targets ti which is Regular (precondition
   lp_is_regular), not DirectoryType. So NoHardLinkToDir's hypothesis
   `ir_vtype ir = DirectoryType` is false for the new edge — the
   property is preserved trivially. *)
Theorem link_preserves_NoHardLinkToDir :
  forall g d name ti,
    WellFormed g ->
    LinkPre g d name ti ->
    NoHardLinkToDir (hard_link g d name ti).
Proof.
  intros g d name ti HWF Hpre.
  destruct HWF as [_ [_ [_ [_ [_ [_ HNHL]]]]]].
  destruct Hpre as [_ _ _ Htgt_exists Hreg].
  unfold NoHardLinkToDir in *.
  intros e1 e2 ir Hin1 Hin2 Hu1 Hu2 Heqi Hfind Hvty.
  apply link_edges in Hin1. apply link_edges in Hin2.
  unfold hard_link, find_inode in Hfind. simpl in Hfind.
  apply increment_link_find_vtype in Hfind.
  destruct Hfind as [ir_old [Hfind_old Hvty_eq]].
  (* Vtype of the old inode = vtype of the new inode (since increment_link
     only changes link_count). So ir_vtype ir_old = DirectoryType too. *)
  rewrite Hvty_eq in Hvty.
  destruct Hin1 as [Ho1 | Hn1]; destruct Hin2 as [Ho2 | Hn2].
  - apply (HNHL e1 e2 ir_old Ho1 Ho2 Hu1 Hu2 Heqi Hfind_old Hvty).
  - (* e1 old, e2 new (mkContains d ti name). ce_ino e2 = ti.
       ce_ino e1 = ti by Heqi. Hreg says find_inode g ti yields Regular.
       But Hvty says DirectoryType. Contradiction. *)
    exfalso. subst e2. simpl in Heqi.
    rewrite Heqi in Hfind_old.
    unfold find_inode in Hfind_old.
    specialize (Hreg ir_old Hfind_old). rewrite Hreg in Hvty. discriminate.
  - exfalso. subst e1. simpl in Heqi.
    (* After subst e1, Hfind_old's `ce_ino e1` became `ti`. *)
    unfold find_inode in Hfind_old.
    specialize (Hreg ir_old Hfind_old). rewrite Hreg in Hvty. discriminate.
  - subst e1 e2. reflexivity.
Qed.

(* Rust impl: `sotfs_ops::link` in sotfs-ops/src/lib.rs.
   Runtime cross-check: tests/invariants_match_coq.rs::
   `link_preserves_well_formed`. *)
Theorem link_preserves_WellFormed :
  forall g d name ti,
    WellFormed g ->
    LinkPre g d name ti ->
    WellFormed (hard_link g d name ti).
Proof.
  intros g d name ti HWF Hpre.
  unfold WellFormed. split; [| split; [| split; [| split; [| split; [| split]]]]].
  - exact (link_preserves_TypeInvariant g d name ti HWF Hpre).
  - exact (link_preserves_LinkCountConsistent g d name ti HWF Hpre).
  - exact (link_preserves_UniqueNamesPerDir g d name ti HWF Hpre).
  - exact (link_preserves_NoDanglingEdges g d name ti HWF Hpre).
  - exact (link_preserves_NoDirCycles g d name ti HWF Hpre).
  - exact (link_preserves_DirHasSelfRef g d name ti HWF).
  - exact (link_preserves_NoHardLinkToDir g d name ti HWF Hpre).
Qed.
