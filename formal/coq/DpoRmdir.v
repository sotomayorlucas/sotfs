(* ===================================================================== *)
(* DpoRmdir.v — DPO rule RMDIR + invariant preservation proofs          *)
(*                                                                       *)
(* Corresponds to:                                                       *)
(*   TLA+:  Rmdir(parent_dir, name) in sotfs_graph.tla                  *)
(*   Rust:  rmdir() in sotfs-ops/src/lib.rs                              *)
(*                                                                       *)
(* The rule removes a directory:                                         *)
(*   - Removes the entry edge (parent_dir, target_ino, name)            *)
(*   - Removes the "." edge (target_dir, target_ino, dot_name)          *)
(*   - Removes the ".." edge (target_dir, parent_ino, dotdot_name)      *)
(*   - Removes the DirRec for target_dir                                 *)
(*   - Removes the InodeRec for target_ino                               *)
(*                                                                       *)
(* Precondition: the directory is empty (no user-name edges from it).    *)
(* ===================================================================== *)

Require Import Coq.Arith.Arith.
Require Import Coq.Lists.List.
Require Import Coq.Bool.Bool.
Require Import Lia.
Import ListNotations.

Require Import SotfsGraph.

(* ===================================================================== *)
(* 1. List removal helpers                                               *)
(* ===================================================================== *)

Definition remove_inode (id : InodeId) (inodes : list InodeRec) : list InodeRec :=
  filter (fun ir => negb (Nat.eqb (ir_id ir) id)) inodes.

Definition remove_dir (id : DirId) (dirs : list DirRec) : list DirRec :=
  filter (fun dr => negb (Nat.eqb (dr_id dr) id)) dirs.

Definition remove_three_edges (e1 e2 e3 : ContainsEdge) (edges : list ContainsEdge)
  : list ContainsEdge :=
  filter (fun e => negb (ce_eqb e e1) && negb (ce_eqb e e2) && negb (ce_eqb e e3))
    edges.

(* ===================================================================== *)
(* 2. Preconditions                                                      *)
(* ===================================================================== *)

Record RmdirPre (g : Graph) (parent_dir : DirId) (name : Name)
  (target_ino : InodeId) (target_dir : DirId) (parent_ino : InodeId)
  : Prop := {
  rmp_dir_exists    : dir_exists g parent_dir;
  rmp_user_name     : is_user_name name;
  rmp_entry_exists  : In (mkContains parent_dir target_ino name) (g_edges g);
  rmp_dot_exists    : In (mkContains target_dir target_ino dot_name) (g_edges g);
  rmp_dotdot_exists : In (mkContains target_dir parent_ino dotdot_name) (g_edges g);
  rmp_is_directory  : forall ir, find_inode g target_ino = Some ir ->
                         ir_vtype ir = DirectoryType;
  rmp_target_exists : inode_exists g target_ino;
  rmp_tdir_exists   : dir_exists g target_dir;
  rmp_tdir_link     : find_dir g target_dir = Some (mkDir target_dir target_ino);
  rmp_empty         : forall e, In e (g_edges g) ->
                        ce_dir e = target_dir ->
                        is_user_name (ce_name e) -> False;
  rmp_parent_ino    : inode_exists g parent_ino;
  rmp_target_ne     : target_dir <> parent_dir;
  rmp_ino_ne        : target_ino <> parent_ino;
  (* GC-LINK-2 + leaf-dir: the only edges in g_edges g that target the
     directory inode `target_ino` are the entry edge and the dot edge.
     Justified by:
     - GC-LINK-2 (Rust): cannot hard-link directories, so the only
       user-name edge to target_ino is the parent's entry edge.
     - DirHasSelfRef + uniqueness of (dr_inode_id) → only one `.` edge.
     - rmp_empty: target_ino is a leaf, so no child has `..` pointing
       to it.
     The Rust caller establishes this before invoking rmdir; the Coq
     formalism accepts it as a precondition rather than deriving it
     from three separate invariants (DotPointsToOwnDir,
     DotdotPointsToParent, NoDupDirInodes). *)
  rmp_only_target_links : forall e, In e (g_edges g) ->
                            ce_ino e = target_ino ->
                            e = mkContains parent_dir target_ino name \/
                            e = mkContains target_dir target_ino dot_name;
}.

(* ===================================================================== *)
(* 3. The rmdir function                                                 *)
(* ===================================================================== *)

Definition rmdir (g : Graph) (parent_dir : DirId) (name : Name)
  (target_ino : InodeId) (target_dir : DirId) (parent_ino : InodeId) : Graph :=
  let entry_edge := mkContains parent_dir target_ino name in
  let dot_edge := mkContains target_dir target_ino dot_name in
  let dotdot_edge := mkContains target_dir parent_ino dotdot_name in
  {| g_inodes := remove_inode target_ino (g_inodes g);
     g_dirs   := remove_dir target_dir (g_dirs g);
     g_edges  := remove_three_edges entry_edge dot_edge dotdot_edge (g_edges g);
  |}.

(* ===================================================================== *)
(* 4. Auxiliary lemmas                                                   *)
(* ===================================================================== *)

Lemma remove_inode_preserves :
  forall id inodes ir,
    In ir inodes -> ir_id ir <> id -> In ir (remove_inode id inodes).
Proof.
  intros id inodes ir Hin Hneq.
  unfold remove_inode. apply filter_In. split.
  - exact Hin.
  - simpl. apply Nat.eqb_neq in Hneq. rewrite Hneq. reflexivity.
Qed.

Lemma remove_inode_subset :
  forall id inodes ir,
    In ir (remove_inode id inodes) -> In ir inodes.
Proof.
  intros. unfold remove_inode in H. apply filter_In in H. tauto.
Qed.

Lemma remove_inode_not_in :
  forall id inodes ir,
    In ir (remove_inode id inodes) -> ir_id ir <> id.
Proof.
  intros id inodes ir Hin.
  unfold remove_inode in Hin. apply filter_In in Hin.
  destruct Hin as [_ Hf]. simpl in Hf.
  destruct (Nat.eqb (ir_id ir) id) eqn:Heq.
  - discriminate.
  - apply Nat.eqb_neq. exact Heq.
Qed.

Lemma remove_dir_preserves :
  forall id dirs dr,
    In dr dirs -> dr_id dr <> id -> In dr (remove_dir id dirs).
Proof.
  intros. unfold remove_dir. apply filter_In. split.
  - exact H.
  - simpl. apply Nat.eqb_neq in H0. rewrite H0. reflexivity.
Qed.

Lemma remove_dir_subset :
  forall id dirs dr,
    In dr (remove_dir id dirs) -> In dr dirs.
Proof.
  intros. unfold remove_dir in H. apply filter_In in H. tauto.
Qed.

Lemma remove_three_edges_subset :
  forall e1 e2 e3 edges e,
    In e (remove_three_edges e1 e2 e3 edges) -> In e edges.
Proof.
  intros. unfold remove_three_edges in H. apply filter_In in H. tauto.
Qed.

Lemma remove_three_edges_not_e1 :
  forall e1 e2 e3 edges e,
    In e (remove_three_edges e1 e2 e3 edges) -> e <> e1.
Proof.
  intros. unfold remove_three_edges in H. apply filter_In in H.
  destruct H as [_ Hf].
  intro Heq. subst e.
  rewrite ce_eqb_refl in Hf. simpl in Hf. discriminate.
Qed.

Lemma remove_three_edges_not_e2 :
  forall e1 e2 e3 edges e,
    In e (remove_three_edges e1 e2 e3 edges) -> e <> e2.
Proof.
  intros. unfold remove_three_edges in H. apply filter_In in H.
  destruct H as [_ Hf].
  intro Heq. subst e.
  rewrite ce_eqb_refl in Hf. simpl in Hf.
  destruct (ce_eqb e2 e1); simpl in Hf; discriminate.
Qed.

Lemma remove_three_edges_not_e3 :
  forall e1 e2 e3 edges e,
    In e (remove_three_edges e1 e2 e3 edges) -> e <> e3.
Proof.
  intros. unfold remove_three_edges in H. apply filter_In in H.
  destruct H as [_ Hf].
  intro Heq. subst e.
  rewrite ce_eqb_refl in Hf.
  destruct (ce_eqb e3 e1); destruct (ce_eqb e3 e2); simpl in Hf; discriminate.
Qed.

Lemma remove_inode_NoDup :
  forall id inodes,
    NoDup (map ir_id inodes) ->
    NoDup (map ir_id (remove_inode id inodes)).
Proof.
  intros id inodes. unfold remove_inode.
  induction inodes as [|h t IH]; intro Hnd.
  - simpl. constructor.
  - simpl. destruct (Nat.eqb (ir_id h) id) eqn:Heq.
    + simpl. apply IH. inversion Hnd. assumption.
    + simpl. constructor.
      * inversion Hnd as [|? ? Hnotin Hnodup_tail].
        intro Hin.
        apply in_map_iff in Hin. destruct Hin as [x0 [Hxid Hxin]].
        apply filter_In in Hxin. destruct Hxin as [Hxin _].
        apply Hnotin. rewrite <- Hxid. apply in_map. exact Hxin.
      * apply IH. inversion Hnd. assumption.
Qed.

Lemma remove_dir_NoDup :
  forall id dirs,
    NoDup (map dr_id dirs) ->
    NoDup (map dr_id (remove_dir id dirs)).
Proof.
  intros id dirs. unfold remove_dir.
  induction dirs as [|h t IH]; intro Hnd.
  - simpl. constructor.
  - simpl. destruct (Nat.eqb (dr_id h) id) eqn:Heq.
    + simpl. apply IH. inversion Hnd. assumption.
    + simpl. constructor.
      * inversion Hnd as [|? ? Hnotin Hnodup_tail].
        intro Hin.
        apply in_map_iff in Hin. destruct Hin as [x0 [Hxid Hxin]].
        apply filter_In in Hxin. destruct Hxin as [Hxin _].
        apply Hnotin. rewrite <- Hxid. apply in_map. exact Hxin.
      * apply IH. inversion Hnd. assumption.
Qed.

(* ===================================================================== *)
(* 5. THEOREM: rmdir preserves TypeInvariant                             *)
(* ===================================================================== *)

Theorem rmdir_preserves_TypeInvariant :
  forall g pd name ti td pi,
    WellFormed g ->
    RmdirPre g pd name ti td pi ->
    TypeInvariant (rmdir g pd name ti td pi).
Proof.
  intros g pd name ti td pi HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct HTI as [Hedge_endpts [HnodupI HnodupD]].
  destruct Hpre as [Hdir Huser He Hdot Hddot Hisdir Htgt Htd Htdlink Hempty Hpi Htne Hine Honly].
  unfold TypeInvariant. split; [| split].
  - (* endpoints exist *)
    intros e0 Hin.
    assert (Hin_orig := remove_three_edges_subset _ _ _ _ _ Hin).
    destruct (HND e0 Hin_orig) as [Hd Hi]. split.
    + (* dir_exists in rmdir graph *)
      unfold dir_exists, dir_ids, rmdir. simpl.
      apply in_map_iff.
      assert (Hce_dir_ne : ce_dir e0 <> td).
      { intro Habs.
        assert (Hne1 := remove_three_edges_not_e1 _ _ _ _ _ Hin).
        assert (Hne2 := remove_three_edges_not_e2 _ _ _ _ _ Hin).
        assert (Hne3 := remove_three_edges_not_e3 _ _ _ _ _ Hin).
        destruct (Nat.eq_dec (ce_name e0) dot_name) as [Hdname | Hndname].
        - apply Hne2.
          assert (Heq := HUN e0 (mkContains td ti dot_name) Hin_orig Hdot).
          rewrite Habs in Heq. specialize (Heq eq_refl Hdname).
          exact Heq.
        - destruct (Nat.eq_dec (ce_name e0) dotdot_name) as [Hddname | Hnddname].
          + apply Hne3.
            assert (Heq := HUN e0 (mkContains td pi dotdot_name) Hin_orig Hddot).
            rewrite Habs in Heq. specialize (Heq eq_refl Hddname).
            exact Heq.
          + apply (Hempty e0 Hin_orig Habs).
            unfold is_user_name, dot_name, dotdot_name in *.
            destruct (ce_name e0) as [| [|n]] eqn:Hcn.
            * exfalso. apply Hndname. reflexivity.
            * exfalso. apply Hnddname. reflexivity.
            * lia. }
      unfold dir_exists, dir_ids in Hd.
      apply in_map_iff in Hd. destruct Hd as [dr [Hdr_id Hdr_in]].
      exists dr. split.
      * exact Hdr_id.
      * apply remove_dir_preserves.
        { exact Hdr_in. }
        { rewrite Hdr_id. exact Hce_dir_ne. }
    + (* inode_exists in rmdir graph *)
      unfold inode_exists, inode_ids, rmdir. simpl.
      assert (Hce_ino_ne : ce_ino e0 <> ti).
      { intro Habs.
        (* By Honly (rmp_only_target_links): the only edges in g_edges g
           with ce_ino = ti are the entry edge and the dot edge. Both are
           removed by rmdir, so e0 (which survived) cannot equal either. *)
        assert (Hne1 := remove_three_edges_not_e1 _ _ _ _ _ Hin).
        assert (Hne2 := remove_three_edges_not_e2 _ _ _ _ _ Hin).
        destruct (Honly e0 Hin_orig Habs) as [Hentry | Hdot_eq].
        - apply Hne1. exact Hentry.
        - apply Hne2. exact Hdot_eq. }
      unfold inode_exists, inode_ids in Hi.
      apply in_map_iff in Hi. destruct Hi as [ir [Hir_id Hir_in]].
      apply in_map_iff. exists ir. split.
      * exact Hir_id.
      * apply remove_inode_preserves; [ exact Hir_in | ].
        rewrite Hir_id. exact Hce_ino_ne.
  - (* NoDupInodeIds *)
    unfold NoDupInodeIds, inode_ids, rmdir. simpl.
    apply remove_inode_NoDup. exact HnodupI.
  - (* NoDupDirIds *)
    unfold NoDupDirIds, dir_ids, rmdir. simpl.
    apply remove_dir_NoDup. exact HnodupD.
Qed.
(* Closed in v0.2.7 using rmp_only_target_links: the only edges
   targeting ti are entry and dot, both removed by rmdir.
   Original baseline note kept for reference: *)
(* The proof is complete except for one sub-case: showing that
   no surviving edge targets the removed inode ti. This requires the
   additional fact that directories cannot be hard-linked (GC-LINK-2),
   so the only incoming edges to ti are the entry edge and the dot edge,
   both of which are removed. A full formalization would add this as
   a precondition (rmp_only_links) or derive it from a NoHardLinkToDir
   invariant added to WellFormed. *)

(* ===================================================================== *)
(* 6. THEOREM: rmdir preserves UniqueNamesPerDir                         *)
(* ===================================================================== *)

(* Removing edges and directories can only make names more unique. *)

Theorem rmdir_preserves_UniqueNamesPerDir :
  forall g pd name ti td pi,
    WellFormed g ->
    RmdirPre g pd name ti td pi ->
    UniqueNamesPerDir (rmdir g pd name ti td pi).
Proof.
  intros g pd name ti td pi HWF Hpre.
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  unfold UniqueNamesPerDir.
  intros e1 e2 Hin1 Hin2 Hdir Hname.
  unfold rmdir in Hin1, Hin2. simpl in Hin1, Hin2.
  apply remove_three_edges_subset in Hin1.
  apply remove_three_edges_subset in Hin2.
  apply HUN; assumption.
Qed.

(* ===================================================================== *)
(* 7. THEOREM: rmdir preserves NoDanglingEdges                           *)
(* ===================================================================== *)

(* This requires the same argument as TypeInvariant — surviving edges
   don't target removed nodes. *)

Theorem rmdir_preserves_NoDanglingEdges :
  forall g pd name ti td pi,
    WellFormed g ->
    RmdirPre g pd name ti td pi ->
    NoDanglingEdges (rmdir g pd name ti td pi).
Proof.
  intros g pd name ti td pi HWF Hpre.
  (* Same structure as TypeInvariant — endpoints of surviving edges
     are not the removed nodes. *)
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct Hpre as [Hdir Huser He Hdot Hddot Hisdir Htgt Htd Htdlink Hempty Hpi Htne Hine Honly].
  unfold NoDanglingEdges.
  intros e0 Hin.
  assert (Hne1 := remove_three_edges_not_e1 _ _ _ _ _ Hin).
  assert (Hne2 := remove_three_edges_not_e2 _ _ _ _ _ Hin).
  assert (Hin_orig := remove_three_edges_subset _ _ _ _ _ Hin).
  destruct (HND e0 Hin_orig) as [Hd_orig Hi_orig].
  (* ce_dir e0 <> td: same argument as in TypeInvariant. Re-prove inline. *)
  assert (Hce_dir_ne : ce_dir e0 <> td).
  { intro Habs.
    assert (Hne3 := remove_three_edges_not_e3 _ _ _ _ _ Hin).
    destruct HTI as [_ [_ _]].
    destruct (Nat.eq_dec (ce_name e0) dot_name) as [Hdname | Hndname].
    - apply Hne2.
      assert (Heq := HUN e0 (mkContains td ti dot_name) Hin_orig Hdot).
      rewrite Habs in Heq. specialize (Heq eq_refl Hdname). exact Heq.
    - destruct (Nat.eq_dec (ce_name e0) dotdot_name) as [Hddname | Hnddname].
      + apply Hne3.
        assert (Heq := HUN e0 (mkContains td pi dotdot_name) Hin_orig Hddot).
        rewrite Habs in Heq. specialize (Heq eq_refl Hddname). exact Heq.
      + apply (Hempty e0 Hin_orig Habs).
        unfold is_user_name, dot_name, dotdot_name in *.
        destruct (ce_name e0) as [| [|n]] eqn:Hcn.
        * exfalso. apply Hndname. reflexivity.
        * exfalso. apply Hnddname. reflexivity.
        * lia. }
  (* ce_ino e0 <> ti: via Honly + Hne1, Hne2. *)
  assert (Hce_ino_ne : ce_ino e0 <> ti).
  { intro Habs.
    destruct (Honly e0 Hin_orig Habs) as [Hentry | Hdot_eq].
    - apply Hne1. exact Hentry.
    - apply Hne2. exact Hdot_eq. }
  split.
  - (* dir_exists in rmdir graph *)
    unfold dir_exists, dir_ids, rmdir. simpl.
    apply in_map_iff.
    unfold dir_exists, dir_ids in Hd_orig.
    apply in_map_iff in Hd_orig. destruct Hd_orig as [dr [Hdr_id Hdr_in]].
    exists dr. split.
    + exact Hdr_id.
    + apply remove_dir_preserves.
      * exact Hdr_in.
      * rewrite Hdr_id. exact Hce_dir_ne.
  - (* inode_exists in rmdir graph *)
    unfold inode_exists, inode_ids, rmdir. simpl.
    unfold inode_exists, inode_ids in Hi_orig.
    apply in_map_iff in Hi_orig. destruct Hi_orig as [ir [Hir_id Hir_in]].
    apply in_map_iff. exists ir. split.
    + exact Hir_id.
    + apply remove_inode_preserves.
      * exact Hir_in.
      * rewrite Hir_id. exact Hce_ino_ne.
Qed.

(* ===================================================================== *)
(* 8. THEOREM: rmdir preserves NoDirCycles                               *)
(* ===================================================================== *)

(* Removing nodes and edges can only break cycles, never create them. *)

Theorem rmdir_preserves_NoDirCycles :
  forall g pd name ti td pi,
    WellFormed g ->
    RmdirPre g pd name ti td pi ->
    NoDirCycles (rmdir g pd name ti td pi).
Proof.
  intros g pd name ti td pi HWF Hpre.
  assert (Hpre_copy := Hpre).
  destruct HWF as [HTI [HLC [HUN [HND [HNC [HDSR HNHL]]]]]].
  destruct HTI as [_ [HnodupI HnodupD]].
  destruct HNC as [rank Hrank].
  destruct Hpre as [Hdir Huser He Hdot Hddot Hisdir Htgt Htd Htdlink
                    Hempty Hpi Htne Hine Honly].
  exists rank.
  intros e0 Hin Huser_name ir Hfind Hvtype child_dir Hchild.
  unfold rmdir in Hin. simpl in Hin.
  assert (Hne1 := remove_three_edges_not_e1 _ _ _ _ _ Hin).
  apply remove_three_edges_subset in Hin.
  (* Lift find_inode (rmdir g) (ce_ino e0) = Some ir back to the old graph
     using the NoDup-based helper. *)
  unfold find_inode in Hfind. unfold rmdir in Hfind. simpl in Hfind.
  unfold remove_inode in Hfind.
  apply find_some in Hfind. destruct Hfind as [Hir_in Hir_eq].
  apply filter_In in Hir_in. destruct Hir_in as [Hir_in_g _].
  apply Nat.eqb_eq in Hir_eq.
  assert (Hfind_g : find_inode g (ce_ino e0) = Some ir).
  { unfold find_inode. apply find_inode_in_NoDup; assumption. }
  (* Use HNHL to rule out ce_ino e0 = ti: in that case e0 would equal the
     entry edge, contradicting Hne1. *)
  assert (Hne_ti : ce_ino e0 <> ti).
  { intro Heq_ti. apply Hne1.
    apply (HNHL e0 (mkContains pd ti name) ir Hin He Huser_name Huser).
    - simpl. exact Heq_ti.
    - simpl. exact Hfind_g.
    - exact Hvtype. }
  (* dir_for_inode (rmdir g) (ce_ino e0) = dir_for_inode g (ce_ino e0):
     the only filtered-out dir is mkDir td ti, whose inode is ti, not
     ce_ino e0 (by Hne_ti). So find on filtered = find on unfiltered. *)
  assert (Hchild_g : dir_for_inode g (ce_ino e0) = Some child_dir).
  { unfold dir_for_inode in Hchild |- *.
    unfold rmdir, remove_dir in Hchild. simpl in Hchild.
    rewrite (find_filter_eq
              (fun dr => Nat.eqb (dr_inode_id dr) (ce_ino e0))
              (fun dr => negb (Nat.eqb (dr_id dr) td))
              (g_dirs g)) in Hchild.
    - exact Hchild.
    - intros x Hx_in Hfx.
      apply negb_false_iff in Hfx. apply Nat.eqb_eq in Hfx.
      (* x's dr_id = td. By NoDupDirIds and Htdlink, x = mkDir td ti. *)
      assert (Hx_eq : x = mkDir td ti).
      { apply find_some in Htdlink. destruct Htdlink as [Htd_in Htd_id_eq].
        apply Nat.eqb_eq in Htd_id_eq.
        apply (NoDup_map_inj dr_id (g_dirs g) x (mkDir td ti) HnodupD
                             Hx_in Htd_in).
        simpl. exact Hfx. }
      subst x. simpl. apply Nat.eqb_neq. intro H. apply Hne_ti.
      symmetry. exact H. }
  apply (Hrank e0 Hin Huser_name ir Hfind_g Hvtype child_dir Hchild_g).
Qed.

(* ===================================================================== *)
(* 9. MAIN THEOREM: rmdir preserves WellFormed (modulo TypeInvariant)    *)
(* ===================================================================== *)

(* The full WellFormed preservation depends on closing the TypeInvariant
   and NoDanglingEdges sub-cases, which require the NoHardLinkToDir
   invariant. We state the theorem with Admitted for transparency. *)

(* rmdir removes target_dir from g_dirs and the dot/dotdot edges. Other
   dirs in (g_dirs g) keep their `.` self-edges (we only removed three
   specific edges, none of which is a dot edge for OTHER dirs). For the
   removed target_dir itself, it's gone from g_dirs, so we don't need to
   prove its `.` survives. The hypothesis quantifies over dirs in the
   NEW g_dirs, which excludes target_dir. *)
Theorem rmdir_preserves_DirHasSelfRef :
  forall g pd name ti td pi,
    WellFormed g ->
    RmdirPre g pd name ti td pi ->
    DirHasSelfRef (rmdir g pd name ti td pi).
Proof.
  intros g pd name ti td pi HWF Hpre.
  destruct HWF as [_ [_ [_ [_ [_ [HDSR _]]]]]].
  destruct Hpre as [_ Huser _ _ _ _ _ _ _ _ _ _ _ _].
  unfold DirHasSelfRef in *.
  intros d0 Hin. unfold rmdir in *. simpl in *.
  (* d0 is in remove_dir td (g_dirs g): preserve dr_id d0 ≠ td via
     filter_In, then show the self-edge survives the three removals. *)
  unfold remove_dir in Hin. apply filter_In in Hin.
  destruct Hin as [Hd0_in_g Hd0_id_ne_b].
  apply negb_true_iff in Hd0_id_ne_b. apply Nat.eqb_neq in Hd0_id_ne_b.
  specialize (HDSR d0 Hd0_in_g).
  (* The self-edge of d0 wasn't entry, dot, or dotdot:
     - entry has name = user_name; self has dot_name (≠).
     - dot has dir = td; self has dir = dr_id d0 ≠ td (Hd0_id_ne_b).
     - dotdot has name = dotdot_name; self has dot_name (≠).
     So it survives remove_three_edges. *)
  unfold remove_three_edges. apply filter_In. split.
  - exact HDSR.
  - simpl.
    (* Three conjuncts: negb (ce_eqb self entry) && negb (self vs dot) &&
       negb (self vs dotdot). All three must be true. *)
    (* Prove the negb-conjuncts individually using boolean reflection. *)
    assert (Hne_e : ce_eqb (mkContains (dr_id d0) (dr_inode_id d0) dot_name)
                          (mkContains pd ti name) = false).
    { apply Bool.not_true_is_false. intro Heq_t.
      apply ce_eqb_eq in Heq_t.
      injection Heq_t as Hdir_eq Hino_eq Hname_eq.
      rewrite <- Hname_eq in Huser.
      unfold is_user_name, dot_name in Huser. lia. }
    assert (Hne_d : ce_eqb (mkContains (dr_id d0) (dr_inode_id d0) dot_name)
                          (mkContains td ti dot_name) = false).
    { apply Bool.not_true_is_false. intro Heq_t.
      apply ce_eqb_eq in Heq_t.
      injection Heq_t as Hdir_eq Hino_eq.
      apply Hd0_id_ne_b. exact Hdir_eq. }
    assert (Hne_dd : ce_eqb (mkContains (dr_id d0) (dr_inode_id d0) dot_name)
                           (mkContains td pi dotdot_name) = false).
    { apply Bool.not_true_is_false. intro Heq_t.
      apply ce_eqb_eq in Heq_t.
      injection Heq_t as Hdir_eq Hino_eq Hname_eq.
      unfold dot_name, dotdot_name in Hname_eq. discriminate. }
    rewrite Hne_e, Hne_d, Hne_dd. reflexivity.
Qed.

(* rmdir removes edges; the new edge set is a subset of the old. So any
   NoHardLinkToDir-violation in the new graph would already be one in
   the old graph — preservation is by subset. We also need find_inode
   to lift, which is straightforward since g_inodes (rmdir) ⊆ g_inodes g. *)
Theorem rmdir_preserves_NoHardLinkToDir :
  forall g pd name ti td pi,
    WellFormed g ->
    RmdirPre g pd name ti td pi ->
    NoHardLinkToDir (rmdir g pd name ti td pi).
Proof.
  intros g pd name ti td pi HWF Hpre.
  destruct HWF as [HTI [_ [_ [_ [_ [_ HNHL]]]]]].
  destruct HTI as [_ [HnodupI _]].
  unfold NoHardLinkToDir in *.
  intros e1 e2 ir Hin1 Hin2 Hu1 Hu2 Heqi Hfind Hvty.
  unfold rmdir in *. simpl in Hin1, Hin2, Hfind.
  apply remove_three_edges_subset in Hin1.
  apply remove_three_edges_subset in Hin2.
  (* Lift find_inode (rmdir g) (ce_ino e1) back to g via filter_In + NoDup. *)
  unfold find_inode in Hfind. unfold remove_inode in Hfind.
  apply find_some in Hfind. destruct Hfind as [Hir_in Hir_id].
  apply filter_In in Hir_in. destruct Hir_in as [Hir_in_g _].
  apply Nat.eqb_eq in Hir_id.
  assert (Hfind_g : find_inode g (ce_ino e1) = Some ir).
  { unfold find_inode. apply find_inode_in_NoDup; assumption. }
  apply (HNHL e1 e2 ir Hin1 Hin2 Hu1 Hu2 Heqi Hfind_g Hvty).
Qed.

(* rmdir removes target inode ti from g_inodes, and three edges: entry
   (targets ti, user_name), dot (targets ti, dot_name), dotdot
   (targets pi, dotdot_name). incoming_count excludes dotdot, so:
   - For ino ≠ ti: only edges targeting ino are kept; the 3 removed
     don't target ino (entry/dot target ti; dotdot has dotdot_name and
     is excluded anyway). So incoming_count is unchanged.
   - For ti: ti is removed from inodes, so LinkCountConsistent doesn't
     quantify over it. *)
Theorem rmdir_preserves_LinkCountConsistent :
  forall g pd name ti td pi,
    WellFormed g ->
    RmdirPre g pd name ti td pi ->
    LinkCountConsistent (rmdir g pd name ti td pi).
Proof.
  intros g pd name ti td pi HWF Hpre.
  destruct HWF as [_ [HLC [_ [_ [_ [_ _]]]]]].
  destruct Hpre as [_ Huser _ _ _ _ _ _ _ _ _ _ _ _].
  unfold LinkCountConsistent in *.
  intros ir Hin. unfold rmdir in Hin. simpl in Hin.
  apply remove_inode_subset in Hin as Hin_g.
  assert (Hir_ne_ti : ir_id ir <> ti).
  { unfold rmdir in *. simpl in *.
    apply remove_inode_not_in in Hin. exact Hin. }
  unfold incoming_count, rmdir. simpl.
  unfold remove_three_edges.
  rewrite count_occ_pred_filter_eq.
  - apply HLC. exact Hin_g.
  - intros x Hx_in Hgx.
    (* The filter pred is false: x is one of entry/dot/dotdot. *)
    apply Bool.andb_false_iff in Hgx as [Hgx | Hgx].
    + apply Bool.andb_false_iff in Hgx as [Hgx | Hgx].
      * apply Bool.negb_false_iff in Hgx.
        apply ce_eqb_eq in Hgx. subst x. simpl.
        (* x = entry; ce_ino = ti ≠ ir_id ir *)
        assert (Hne : Nat.eqb ti (ir_id ir) = false).
        { apply Nat.eqb_neq. intro H. apply Hir_ne_ti. symmetry. exact H. }
        rewrite Hne. reflexivity.
      * apply Bool.negb_false_iff in Hgx.
        apply ce_eqb_eq in Hgx. subst x. simpl.
        assert (Hne : Nat.eqb ti (ir_id ir) = false).
        { apply Nat.eqb_neq. intro H. apply Hir_ne_ti. symmetry. exact H. }
        rewrite Hne. reflexivity.
    + apply Bool.negb_false_iff in Hgx.
      apply ce_eqb_eq in Hgx. subst x. simpl.
      (* x = dotdot; predicate has negb (dotdot_name =? dotdot_name) = false. *)
      destruct (Nat.eqb dotdot_name dotdot_name) eqn:Hdd.
      * simpl. rewrite Bool.andb_false_r. reflexivity.
      * rewrite Nat.eqb_refl in Hdd. discriminate.
Qed.

Theorem rmdir_preserves_WellFormed :
  forall g pd name ti td pi,
    WellFormed g ->
    RmdirPre g pd name ti td pi ->
    WellFormed (rmdir g pd name ti td pi).
Proof.
  intros g pd name ti td pi HWF Hpre.
  unfold WellFormed. split; [| split; [| split; [| split; [| split; [| split]]]]].
  - exact (rmdir_preserves_TypeInvariant g pd name ti td pi HWF Hpre).
  - exact (rmdir_preserves_LinkCountConsistent g pd name ti td pi HWF Hpre).
  - exact (rmdir_preserves_UniqueNamesPerDir g pd name ti td pi HWF Hpre).
  - exact (rmdir_preserves_NoDanglingEdges g pd name ti td pi HWF Hpre).
  - exact (rmdir_preserves_NoDirCycles g pd name ti td pi HWF Hpre).
  - exact (rmdir_preserves_DirHasSelfRef g pd name ti td pi HWF Hpre).
  - exact (rmdir_preserves_NoHardLinkToDir g pd name ti td pi HWF Hpre).
Qed.
(* NOTE: The admits are for:
   1. TypeInvariant/NoDanglingEdges: proving no surviving edge targets
      the removed inode (requires NoHardLinkToDir invariant)
   2. LinkCountConsistent: counting argument that removing 2 non-dotdot
      edges (entry + dot) and the inode itself preserves the invariant
      for all remaining inodes.
   These are mechanically involved but conceptually straightforward.
   The NoDirCycles and UniqueNamesPerDir proofs are complete. *)
