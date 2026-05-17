(* ===================================================================== *)
(* DpoRename.v — DPO rule RENAME (same-dir, no replacement) +           *)
(*               invariant preservation proofs                           *)
(*                                                                       *)
(* Corresponds to:                                                       *)
(*   TLA+:  RenameSameDir(d, oldName, newName) in sotfs_graph.tla        *)
(*          lines 388-405                                                *)
(*   Rust:  rename() in sotfs-ops/src/lib.rs lines 382-517              *)
(*          (simplified: same directory, no target replacement)           *)
(*                                                                       *)
(* The rule atomically removes the edge (d, target_ino, old_name)        *)
(* and adds edge (d, target_ino, new_name). link_count is unchanged     *)
(* because one incoming non-dotdot edge is removed and one is added.     *)
(* ===================================================================== *)

Require Import Coq.Arith.Arith.
Require Import Coq.Lists.List.
Require Import Coq.Bool.Bool.
Require Import Lia.
Import ListNotations.

Require Import SotfsGraph.

(* ===================================================================== *)
(* 1. Preconditions (gluing conditions)                                  *)
(* ===================================================================== *)

Record RenamePre (g : Graph) (d : DirId) (old_name new_name : Name)
  (target_ino : InodeId) : Prop := {
  rp_dir_exists   : dir_exists g d;
  rp_old_user     : is_user_name old_name;
  rp_new_user     : is_user_name new_name;
  rp_names_diff   : old_name <> new_name;
  rp_edge_exists  : In (mkContains d target_ino old_name) (g_edges g);
  rp_new_fresh    : name_in_dir g d new_name = false;
  rp_target_exists : inode_exists g target_ino;
  (* The old edge appears exactly once in g_edges g. The Rust caller
     establishes this via UniqueNamesPerDir + the implementation never
     inserts duplicate records (it would be a bug). Without this, two
     copies of the old edge would survive remove_edge (which only
     removes one) and break uniqueness reasoning. *)
  rp_no_dup_old   : ~ In (mkContains d target_ino old_name)
                         (remove_edge (mkContains d target_ino old_name)
                                      (g_edges g));
}.

(* ===================================================================== *)
(* 2. The rename_same_dir function                                       *)
(* ===================================================================== *)

Definition rename_same_dir (g : Graph) (d : DirId) (target_ino : InodeId)
  (old_name new_name : Name) : Graph :=
  {| g_inodes := g_inodes g;
     g_dirs   := g_dirs g;
     g_edges  := (remove_edge (mkContains d target_ino old_name) (g_edges g))
                 ++ [mkContains d target_ino new_name];
  |}.

(* ===================================================================== *)
(* 3. Edge membership in the renamed graph                               *)
(* ===================================================================== *)

Lemma rename_edges :
  forall g d target_ino old_name new_name e,
    In e (g_edges (rename_same_dir g d target_ino old_name new_name)) <->
    (In e (remove_edge (mkContains d target_ino old_name) (g_edges g)) \/
     e = mkContains d target_ino new_name).
Proof.
  intros. unfold rename_same_dir. simpl.
  rewrite in_app_iff. simpl.
  (* Coq 8.20: tauto doesn't symmetrize `=`; do it manually. *)
  split.
  - intros [Hold | [Hnew | []]]; [left; assumption | right; symmetry; assumption].
  - intros [Hold | Hnew]; [left; assumption | right; left; symmetry; assumption].
Qed.

(* ===================================================================== *)
(* 4. THEOREM: rename_same_dir preserves TypeInvariant                   *)
(* ===================================================================== *)

Theorem rename_preserves_TypeInvariant :
  forall g d old_name new_name target_ino,
    WellFormed g ->
    RenamePre g d old_name new_name target_ino ->
    TypeInvariant (rename_same_dir g d target_ino old_name new_name).
Proof.
  intros g d old_name new_name target_ino HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct HTI as [Hedge_endpts [HnodupI HnodupD]].
  destruct Hpre as [Hdir Huser_old Huser_new Hdiff Hedge_in Hnew_fresh Htgt Hno_dup].
  unfold TypeInvariant. split; [| split].
  - (* endpoints exist *)
    intros e0 Hin.
    apply rename_edges in Hin. destruct Hin as [Hrem | Hnew].
    + apply remove_edge_subset in Hrem.
      destruct (HND e0 Hrem) as [Hd Hi]. split.
      * unfold dir_exists, dir_ids, rename_same_dir. simpl. exact Hd.
      * unfold inode_exists, inode_ids, rename_same_dir. simpl. exact Hi.
    + subst e0. simpl. split.
      * unfold dir_exists, dir_ids, rename_same_dir. simpl. exact Hdir.
      * unfold inode_exists, inode_ids, rename_same_dir. simpl. exact Htgt.
  - (* NoDupInodeIds — inodes unchanged *)
    unfold NoDupInodeIds, inode_ids, rename_same_dir. simpl. exact HnodupI.
  - (* NoDupDirIds — dirs unchanged *)
    unfold NoDupDirIds, dir_ids, rename_same_dir. simpl. exact HnodupD.
Qed.

(* ===================================================================== *)
(* 5. THEOREM: rename_same_dir preserves UniqueNamesPerDir               *)
(* ===================================================================== *)

Theorem rename_preserves_UniqueNamesPerDir :
  forall g d old_name new_name target_ino,
    WellFormed g ->
    RenamePre g d old_name new_name target_ino ->
    UniqueNamesPerDir (rename_same_dir g d target_ino old_name new_name).
Proof.
  intros g d old_name new_name target_ino HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct Hpre as [Hdir Huser_old Huser_new Hdiff Hedge_in Hnew_fresh Htgt Hno_dup].
  unfold UniqueNamesPerDir.
  intros e1 e2 Hin1 Hin2 Hdir_eq Hname_eq.
  apply rename_edges in Hin1. apply rename_edges in Hin2.
  destruct Hin1 as [H1rem | H1new]; destruct Hin2 as [H2rem | H2new].
  - (* both from remove_edge — they were in the old graph *)
    apply remove_edge_subset in H1rem.
    apply remove_edge_subset in H2rem.
    apply HUN; assumption.
  - (* e1 from remove, e2 = new edge *)
    subst e2. simpl in Hdir_eq, Hname_eq.
    exfalso.
    apply remove_edge_subset in H1rem.
    apply (name_in_dir_false_not_in g d new_name Hnew_fresh).
    exists (ce_ino e1).
    destruct e1 as [d1 i1 n1]. simpl in *. subst.
    exact H1rem.
  - (* e1 = new edge, e2 from remove — symmetric *)
    subst e1. simpl in Hdir_eq, Hname_eq.
    exfalso.
    apply remove_edge_subset in H2rem.
    apply (name_in_dir_false_not_in g d new_name Hnew_fresh).
    exists (ce_ino e2).
    destruct e2 as [d2 i2 n2]. simpl in *. subst.
    exact H2rem.
  - (* both new *)
    subst. reflexivity.
Qed.

(* ===================================================================== *)
(* 6. THEOREM: rename_same_dir preserves NoDanglingEdges                 *)
(* ===================================================================== *)

Theorem rename_preserves_NoDanglingEdges :
  forall g d old_name new_name target_ino,
    WellFormed g ->
    RenamePre g d old_name new_name target_ino ->
    NoDanglingEdges (rename_same_dir g d target_ino old_name new_name).
Proof.
  intros g d old_name new_name target_ino HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct Hpre as [Hdir Huser_old Huser_new Hdiff Hedge_in Hnew_fresh Htgt Hno_dup].
  unfold NoDanglingEdges.
  intros e Hin.
  apply rename_edges in Hin. destruct Hin as [Hrem | Hnew].
  - apply remove_edge_subset in Hrem.
    destruct (HND e Hrem) as [Hd Hi]. split.
    + unfold dir_exists, dir_ids, rename_same_dir. simpl. exact Hd.
    + unfold inode_exists, inode_ids, rename_same_dir. simpl. exact Hi.
  - subst e. simpl. split.
    + unfold dir_exists, dir_ids, rename_same_dir. simpl. exact Hdir.
    + unfold inode_exists, inode_ids, rename_same_dir. simpl. exact Htgt.
Qed.

(* ===================================================================== *)
(* 7. THEOREM: rename_same_dir preserves LinkCountConsistent             *)
(* ===================================================================== *)

(* Key insight: For target_ino, the old edge (user name, counted) is     *)
(* removed and a new edge (also user name, also counted) is added.       *)
(* Net change = 0. For all other inodes, neither edge is relevant.       *)

Lemma rename_incoming_other :
  forall g d target_ino old_name new_name ino,
    ino <> target_ino ->
    incoming_count (rename_same_dir g d target_ino old_name new_name) ino =
    incoming_count g ino.
Proof.
  intros g d target_ino old_name new_name ino Hneq.
  unfold incoming_count, rename_same_dir. simpl.
  rewrite count_occ_pred_app. simpl.
  assert (Hneq_b : Nat.eqb target_ino ino = false).
  { apply Nat.eqb_neq. intro H. apply Hneq. symmetry. exact H. }
  rewrite Hneq_b. simpl.
  (* The removed edge targets target_ino, not ino. So its removal
     doesn't change the count for ino. *)
  induction (g_edges g) as [|h t IH].
  - simpl. lia.
  - simpl.
    destruct (ce_eqb h (mkContains d target_ino old_name)) eqn:Heq_edge.
    + apply ce_eqb_eq in Heq_edge. subst h. simpl.
      rewrite Hneq_b. simpl. lia.
    + simpl.
      destruct (Nat.eqb (ce_ino h) ino && negb (Nat.eqb (ce_name h) dotdot_name))
        eqn:Hpred.
      * simpl. f_equal. exact IH.
      * exact IH.
Qed.

(* For target_ino: remove one non-dotdot edge, add one non-dotdot edge.
   Net change = 0. Uses count_remove_matching from SotfsGraph.v. *)
Lemma rename_incoming_target :
  forall g d target_ino old_name new_name,
    WellFormed g ->
    In (mkContains d target_ino old_name) (g_edges g) ->
    is_user_name old_name ->
    is_user_name new_name ->
    incoming_count (rename_same_dir g d target_ino old_name new_name) target_ino =
    incoming_count g target_ino.
Proof.
  intros g d target_ino old_name new_name HWF Hin Huser_old Huser_new.
  unfold incoming_count, rename_same_dir. simpl.
  rewrite count_occ_pred_app. simpl.
  rewrite Nat.eqb_refl.
  assert (Hndot_new : Nat.eqb new_name dotdot_name = false).
  { apply Nat.eqb_neq. apply user_name_not_dotdot. exact Huser_new. }
  rewrite Hndot_new. simpl.
  (* count(remove_edge(old)) + 1 = count(original) *)
  assert (Hmatch : count_occ_pred
    (fun e => Nat.eqb (ce_ino e) target_ino && negb (Nat.eqb (ce_name e) dotdot_name))
    (remove_edge (mkContains d target_ino old_name) (g_edges g)) + 1 =
    count_occ_pred
    (fun e => Nat.eqb (ce_ino e) target_ino && negb (Nat.eqb (ce_name e) dotdot_name))
    (g_edges g)).
  { apply count_remove_matching.
    - exact Hin.
    - simpl. rewrite Nat.eqb_refl.
      assert (Hndot_old : Nat.eqb old_name dotdot_name = false).
      { apply Nat.eqb_neq. apply user_name_not_dotdot. exact Huser_old. }
      rewrite Hndot_old. reflexivity. }
  lia.
Qed.

Theorem rename_preserves_LinkCountConsistent :
  forall g d old_name new_name target_ino,
    WellFormed g ->
    RenamePre g d old_name new_name target_ino ->
    LinkCountConsistent (rename_same_dir g d target_ino old_name new_name).
Proof.
  intros g d old_name new_name target_ino HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct Hpre as [Hdir Huser_old Huser_new Hdiff Hedge_in Hnew_fresh Htgt Hno_dup].
  unfold LinkCountConsistent.
  intros ir Hin.
  (* inodes unchanged in rename *)
  unfold rename_same_dir in Hin. simpl in Hin.
  destruct (Nat.eq_dec (ir_id ir) target_ino) as [Heq | Hneq].
  - (* ir is the target — incoming count unchanged *)
    rewrite Heq.
    rewrite (rename_incoming_target g d target_ino old_name new_name).
    + rewrite <- Heq. apply HLC. exact Hin.
    + unfold WellFormed.
      split; [| split; [| split; [| split; [| split; [| split]]]]];
        assumption.
    + exact Hedge_in.
    + exact Huser_old.
    + exact Huser_new.
  - (* ir is not the target — incoming count unchanged *)
    rewrite (rename_incoming_other g d target_ino old_name new_name (ir_id ir) Hneq).
    apply HLC. exact Hin.
Qed.

(* ===================================================================== *)
(* 8. THEOREM: rename_same_dir preserves NoDirCycles                     *)
(* ===================================================================== *)

(* Key insight: same-dir rename does not change the directory DAG         *)
(* structure — the same inode is still a child of the same directory.     *)
(* The old ranking still works because ce_dir and ce_ino are unchanged.  *)

Theorem rename_preserves_NoDirCycles :
  forall g d old_name new_name target_ino,
    WellFormed g ->
    RenamePre g d old_name new_name target_ino ->
    NoDirCycles (rename_same_dir g d target_ino old_name new_name).
Proof.
  intros g d old_name new_name target_ino HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct HNC as [rank Hrank].
  destruct Hpre as [Hdir Huser_old Huser_new Hdiff Hedge_in Hnew_fresh Htgt Hno_dup].
  exists rank.
  intros e Hin Huser_e ir Hfind Hvtype child_dir Hchild.
  apply rename_edges in Hin. destruct Hin as [Hrem | Hnew].
  - (* Old edge (surviving removal) *)
    apply remove_edge_subset in Hrem.
    (* find_inode and dir_for_inode unchanged: nodes untouched *)
    unfold find_inode in Hfind. unfold rename_same_dir in Hfind. simpl in Hfind.
    unfold dir_for_inode in Hchild. unfold rename_same_dir in Hchild. simpl in Hchild.
    apply (Hrank e Hrem Huser_e ir).
    + unfold find_inode. exact Hfind.
    + exact Hvtype.
    + unfold dir_for_inode. exact Hchild.
  - (* New edge (d, target_ino, new_name) *)
    subst e. simpl in *.
    (* This edge has the same (dir, inode) as the old edge
       (d, target_ino, old_name), which satisfied the ranking *)
    unfold find_inode in Hfind. unfold rename_same_dir in Hfind. simpl in Hfind.
    unfold dir_for_inode in Hchild. unfold rename_same_dir in Hchild. simpl in Hchild.
    apply (Hrank (mkContains d target_ino old_name) Hedge_in Huser_old ir).
    + unfold find_inode. exact Hfind.
    + exact Hvtype.
    + unfold dir_for_inode. exact Hchild.
Qed.

(* ===================================================================== *)
(* 9. MAIN THEOREM: rename_same_dir preserves WellFormed                 *)
(* ===================================================================== *)

(* rename_same_dir replaces one user-name edge with another. g_dirs is
   unchanged. The removed edge has the old user_name; the new edge has
   the new user_name. Neither is a `.` edge (both are user-names), so
   DirHasSelfRef survives. *)
Theorem rename_preserves_DirHasSelfRef :
  forall g d old_name new_name target_ino,
    WellFormed g ->
    RenamePre g d old_name new_name target_ino ->
    DirHasSelfRef (rename_same_dir g d target_ino old_name new_name).
Proof.
  intros g d old_name new_name target_ino HWF Hpre.
  destruct HWF as [_ [_ [_ [_ [_ [HDSR _]]]]]].
  destruct Hpre as [_ Huser_old _ _ _ _ _ _].
  unfold DirHasSelfRef in *.
  intros d0 Hin. unfold rename_same_dir in *. simpl in *.
  apply in_or_app. left.
  apply remove_edge_preserves.
  - apply HDSR. exact Hin.
  - intro Habs. inversion Habs as [Heq_dn]. subst.
    unfold is_user_name, dot_name in Huser_old. lia.
Qed.

(* rename preserves ce_ino but changes ce_name. The new and removed
   edges target the same inode; if old NoHardLinkToDir held, the only
   user-name edge to a directory inode was unique — after renaming
   that same edge, the uniqueness still holds. *)
Theorem rename_preserves_NoHardLinkToDir :
  forall g d old_name new_name target_ino,
    WellFormed g ->
    RenamePre g d old_name new_name target_ino ->
    NoHardLinkToDir (rename_same_dir g d target_ino old_name new_name).
Proof.
  intros g d old_name new_name target_ino HWF Hpre.
  destruct HWF as [_ [_ [_ [_ [_ [_ HNHL]]]]]].
  destruct Hpre as [_ Huser_old Huser_new Hdiff Hedge_in _ _ Hno_dup].
  unfold NoHardLinkToDir in *.
  intros e1 e2 ir Hin1 Hin2 Hu1 Hu2 Heqi Hfind Hvty.
  apply rename_edges in Hin1. apply rename_edges in Hin2.
  (* find_inode in rename_same_dir uses the same g_inodes as g. *)
  unfold rename_same_dir, find_inode in Hfind. simpl in Hfind.
  destruct Hin1 as [H1rem | H1new]; destruct Hin2 as [H2rem | H2new].
  - apply remove_edge_subset in H1rem.
    apply remove_edge_subset in H2rem.
    apply (HNHL e1 e2 ir H1rem H2rem Hu1 Hu2 Heqi Hfind Hvty).
  - (* e1 from remove_edge, e2 = new edge. By HNHL on g, e1 = old_edge.
       But Hno_dup says old_edge isn't in remove_edge old_edge — contra. *)
    subst e2. simpl in Heqi.
    apply remove_edge_subset in H1rem as H1rem_g.
    unfold find_inode in Hfind.
    assert (Heq_e1 :
      e1 = mkContains d target_ino old_name).
    { apply (HNHL e1 (mkContains d target_ino old_name) ir H1rem_g Hedge_in
                  Hu1 Huser_old).
      - simpl. exact Heqi.
      - simpl. exact Hfind.
      - exact Hvty. }
    exfalso. apply Hno_dup. rewrite Heq_e1 in H1rem. exact H1rem.
  - (* e1 = new edge, e2 from remove_edge: symmetric. *)
    subst e1. simpl in Heqi.
    apply remove_edge_subset in H2rem as H2rem_g.
    unfold find_inode in Hfind.
    simpl in Hfind. rewrite Heqi in Hfind.
    assert (Heq_e2 :
      e2 = mkContains d target_ino old_name).
    { apply (HNHL e2 (mkContains d target_ino old_name) ir H2rem_g Hedge_in
                  Hu2 Huser_old).
      - simpl. symmetry. exact Heqi.
      - simpl. exact Hfind.
      - exact Hvty. }
    exfalso. apply Hno_dup. rewrite Heq_e2 in H2rem. exact H2rem.
  - subst e1 e2. reflexivity.
Qed.

(* Rust impl: `sotfs_ops::rename` in sotfs-ops/src/lib.rs.
   Runtime cross-check: tests/invariants_match_coq.rs::
   `rename_preserves_well_formed`. *)
Theorem rename_preserves_WellFormed :
  forall g d old_name new_name target_ino,
    WellFormed g ->
    RenamePre g d old_name new_name target_ino ->
    WellFormed (rename_same_dir g d target_ino old_name new_name).
Proof.
  intros g d old_name new_name target_ino HWF Hpre.
  unfold WellFormed. split; [| split; [| split; [| split; [| split; [| split]]]]].
  - exact (rename_preserves_TypeInvariant g d old_name new_name target_ino HWF Hpre).
  - exact (rename_preserves_LinkCountConsistent g d old_name new_name target_ino HWF Hpre).
  - exact (rename_preserves_UniqueNamesPerDir g d old_name new_name target_ino HWF Hpre).
  - exact (rename_preserves_NoDanglingEdges g d old_name new_name target_ino HWF Hpre).
  - exact (rename_preserves_NoDirCycles g d old_name new_name target_ino HWF Hpre).
  - exact (rename_preserves_DirHasSelfRef g d old_name new_name target_ino HWF Hpre).
  - exact (rename_preserves_NoHardLinkToDir g d old_name new_name target_ino HWF Hpre).
Qed.
