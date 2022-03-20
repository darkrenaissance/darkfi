; ModuleID = 'probe1.56ebf6f4-cgu.0'
source_filename = "probe1.56ebf6f4-cgu.0"
target datalayout = "e-m:w-p270:32:32-p271:32:32-p272:64:64-i64:64-f80:128-n8:16:32:64-S128"
target triple = "x86_64-pc-windows-msvc"

%"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>" = type { %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>" }
%"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>" = type { i64, { i32, i32 }, i8, [7 x i8] }
%"core::panic::location::Location" = type { { [0 x i8]*, i64 }, i32, i32 }

@alloc2 = private unnamed_addr constant <{ [27 x i8] }> <{ [27 x i8] c"assertion failed: step != 0" }>, align 1
@alloc3 = private unnamed_addr constant <{ [89 x i8] }> <{ [89 x i8] c"/rustc/f1edd0429582dd29cccacaf50fd134b05593bd9c\\library\\core\\src\\iter\\adapters\\step_by.rs" }>, align 1
@alloc4 = private unnamed_addr constant <{ i8*, [16 x i8] }> <{ i8* getelementptr inbounds (<{ [89 x i8] }>, <{ [89 x i8] }>* @alloc3, i32 0, i32 0, i32 0), [16 x i8] c"Y\00\00\00\00\00\00\00\15\00\00\00\09\00\00\00" }>, align 8

; core::iter::traits::iterator::Iterator::rev
; Function Attrs: inlinehint uwtable
define void @_ZN4core4iter6traits8iterator8Iterator3rev17h82800ac9f6e12da0E(%"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>"* noalias nocapture sret(%"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>") dereferenceable(24) %0, %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* noalias nocapture dereferenceable(24) %self) unnamed_addr #0 {
start:
  %_2 = alloca %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>", align 8
  %1 = bitcast %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* %_2 to i8*
  %2 = bitcast %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* %self to i8*
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* align 8 %1, i8* align 8 %2, i64 24, i1 false)
; call core::iter::adapters::rev::Rev<T>::new
  call void @"_ZN4core4iter8adapters3rev12Rev$LT$T$GT$3new17h09abde7c1d240ee3E"(%"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>"* noalias nocapture sret(%"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>") dereferenceable(24) %0, %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* noalias nocapture dereferenceable(24) %_2)
  br label %bb1

bb1:                                              ; preds = %start
  ret void
}

; core::iter::traits::iterator::Iterator::step_by
; Function Attrs: inlinehint uwtable
define void @_ZN4core4iter6traits8iterator8Iterator7step_by17hf5172c96d9372880E(%"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* noalias nocapture sret(%"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>") dereferenceable(24) %0, i32 %self.0, i32 %self.1, i64 %step) unnamed_addr #0 {
start:
; call core::iter::adapters::step_by::StepBy<I>::new
  call void @"_ZN4core4iter8adapters7step_by15StepBy$LT$I$GT$3new17hc07e14aae832b258E"(%"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* noalias nocapture sret(%"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>") dereferenceable(24) %0, i32 %self.0, i32 %self.1, i64 %step)
  br label %bb1

bb1:                                              ; preds = %start
  ret void
}

; core::iter::adapters::rev::Rev<T>::new
; Function Attrs: uwtable
define void @"_ZN4core4iter8adapters3rev12Rev$LT$T$GT$3new17h09abde7c1d240ee3E"(%"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>"* noalias nocapture sret(%"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>") dereferenceable(24) %0, %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* noalias nocapture dereferenceable(24) %iter) unnamed_addr #1 {
start:
  %_2 = alloca %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>", align 8
  %1 = bitcast %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* %_2 to i8*
  %2 = bitcast %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* %iter to i8*
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* align 8 %1, i8* align 8 %2, i64 24, i1 false)
  %3 = bitcast %"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>"* %0 to %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"*
  %4 = bitcast %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* %3 to i8*
  %5 = bitcast %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* %_2 to i8*
  call void @llvm.memcpy.p0i8.p0i8.i64(i8* align 8 %4, i8* align 8 %5, i64 24, i1 false)
  ret void
}

; core::iter::adapters::step_by::StepBy<I>::new
; Function Attrs: uwtable
define void @"_ZN4core4iter8adapters7step_by15StepBy$LT$I$GT$3new17hc07e14aae832b258E"(%"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* noalias nocapture sret(%"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>") dereferenceable(24) %0, i32 %iter.0, i32 %iter.1, i64 %step) unnamed_addr #1 personality i32 (...)* @__CxxFrameHandler3 {
start:
  %_4 = icmp ne i64 %step, 0
  %_3 = xor i1 %_4, true
  br i1 %_3, label %bb1, label %bb2

bb2:                                              ; preds = %start
  %_7 = sub i64 %step, 1
  %1 = getelementptr inbounds %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>", %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* %0, i32 0, i32 1
  %2 = getelementptr inbounds { i32, i32 }, { i32, i32 }* %1, i32 0, i32 0
  store i32 %iter.0, i32* %2, align 8
  %3 = getelementptr inbounds { i32, i32 }, { i32, i32 }* %1, i32 0, i32 1
  store i32 %iter.1, i32* %3, align 4
  %4 = bitcast %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* %0 to i64*
  store i64 %_7, i64* %4, align 8
  %5 = getelementptr inbounds %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>", %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* %0, i32 0, i32 2
  store i8 1, i8* %5, align 8
  ret void

bb1:                                              ; preds = %start
; invoke core::panicking::panic
  invoke void @_ZN4core9panicking5panic17hc36befec1b90302cE([0 x i8]* nonnull align 1 bitcast (<{ [27 x i8] }>* @alloc2 to [0 x i8]*), i64 27, %"core::panic::location::Location"* align 8 dereferenceable(24) bitcast (<{ i8*, [16 x i8] }>* @alloc4 to %"core::panic::location::Location"*)) #5
          to label %unreachable unwind label %funclet_bb3

unreachable:                                      ; preds = %bb1
  unreachable

bb3:                                              ; preds = %funclet_bb3
  br label %bb4

funclet_bb3:                                      ; preds = %bb1
  %cleanuppad = cleanuppad within none []
  br label %bb3

bb4:                                              ; preds = %bb3
  cleanupret from %cleanuppad unwind to caller
}

; probe1::probe
; Function Attrs: uwtable
define void @_ZN6probe15probe17h4c635ac760564483E() unnamed_addr #1 {
start:
  %_3 = alloca { i32, i32 }, align 4
  %_2 = alloca %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>", align 8
  %_1 = alloca %"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>", align 8
  %0 = bitcast { i32, i32 }* %_3 to i32*
  store i32 0, i32* %0, align 4
  %1 = getelementptr inbounds { i32, i32 }, { i32, i32 }* %_3, i32 0, i32 1
  store i32 10, i32* %1, align 4
  %2 = getelementptr inbounds { i32, i32 }, { i32, i32 }* %_3, i32 0, i32 0
  %3 = load i32, i32* %2, align 4
  %4 = getelementptr inbounds { i32, i32 }, { i32, i32 }* %_3, i32 0, i32 1
  %5 = load i32, i32* %4, align 4
; call core::iter::traits::iterator::Iterator::step_by
  call void @_ZN4core4iter6traits8iterator8Iterator7step_by17hf5172c96d9372880E(%"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* noalias nocapture sret(%"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>") dereferenceable(24) %_2, i32 %3, i32 %5, i64 2)
  br label %bb1

bb1:                                              ; preds = %start
; call core::iter::traits::iterator::Iterator::rev
  call void @_ZN4core4iter6traits8iterator8Iterator3rev17h82800ac9f6e12da0E(%"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>"* noalias nocapture sret(%"core::iter::adapters::rev::Rev<core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>>") dereferenceable(24) %_1, %"core::iter::adapters::step_by::StepBy<core::ops::range::Range<i32>>"* noalias nocapture dereferenceable(24) %_2)
  br label %bb2

bb2:                                              ; preds = %bb1
  ret void
}

; Function Attrs: argmemonly nofree nounwind willreturn
declare void @llvm.memcpy.p0i8.p0i8.i64(i8* noalias nocapture writeonly, i8* noalias nocapture readonly, i64, i1 immarg) #2

declare i32 @__CxxFrameHandler3(...) unnamed_addr #3

; core::panicking::panic
; Function Attrs: cold noinline noreturn uwtable
declare void @_ZN4core9panicking5panic17hc36befec1b90302cE([0 x i8]* nonnull align 1, i64, %"core::panic::location::Location"* align 8 dereferenceable(24)) unnamed_addr #4

attributes #0 = { inlinehint uwtable "target-cpu"="x86-64" }
attributes #1 = { uwtable "target-cpu"="x86-64" }
attributes #2 = { argmemonly nofree nounwind willreturn }
attributes #3 = { "target-cpu"="x86-64" }
attributes #4 = { cold noinline noreturn uwtable "target-cpu"="x86-64" }
attributes #5 = { noreturn }

!llvm.module.flags = !{!0}

!0 = !{i32 7, !"PIC Level", i32 2}
