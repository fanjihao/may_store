// API 层 - 菜品 CRUD 路由
// FSD §5 / 13.3.3 / API doc part1 §5 compliant
//
// 6 个端点:
//   POST   /api/groups/{group_id}/foods                    创建菜品
//   GET    /api/groups/{group_id}/foods                    列表(游标分页)
//   GET    /api/groups/{group_id}/foods/{food_id}          详情
//   PATCH  /api/groups/{group_id}/foods/{food_id}          更新
//   DELETE /api/groups/{group_id}/foods/{food_id}          软删除(is_del=1)
//   POST   /api/groups/{group_id}/foods/{food_id}/hide     切换隐藏
//
// 状态映射:DB 层 food_status (NORMAL/OFF/AUDITING/REJECTED) + is_del
//          API 层 status ("ACTIVE"/"HIDDEN"/"DELETED"/"AUDITING"/"REJECTED")
//          NORMAL ↔ ACTIVE,OFF ↔ HIDDEN,is_del=1 ↔ DELETED

pub mod routes;
